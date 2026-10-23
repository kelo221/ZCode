import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  WorktreeIsolationError,
  createSessionId,
  createTraceId,
  createTurnId,
  type ExecutionPort,
  type ExecutionRequest,
  type ExecutionResult,
  type CodeSearchPort,
  type SessionStorePort,
  type WorktreeFinalizeResult,
  type WorktreeIsolationPort,
  type WorktreeRecord,
} from "@zcode/contracts";
import { parseAgentProfileFromMarkdown } from "@zcode/shared";
import { createTestAgentRuntime } from "./test-agent-runtime.js";
import { createTestSessionEventStore } from "./test-event-store.js";
import { createTestModelFactory } from "./test-runtime-model.js";
import { createRecordingSubagentSessionStore } from "./subagent-test-store.js";
import { MemoryFileSystem } from "./memory-test-utils.js";
import { readAgentMetadataSnapshot } from "../src/subagent/agent-index.js";
import { listAgentsToolEntry } from "../src/tool/handlers/list-agents.js";

const workspace = resolve("/workspace/project");
const RESULT_HEAD = "b".repeat(40);

type ModelStep = (text: string) => Promise<ModelReply> | ModelReply;
interface ModelReply {
  text?: string;
  toolCalls?: Array<{ id: string; name: string; input: Record<string, unknown> }>;
}

function deferred() {
  let resolvePromise!: () => void;
  const promise = new Promise<void>((done) => {
    resolvePromise = done;
  });
  return { promise, resolve: resolvePromise };
}

async function waitFor(check: () => boolean | Promise<boolean>, timeoutMs = 5_000) {
  const started = Date.now();
  while (!(await check())) {
    if (Date.now() - started > timeoutMs) throw new Error("condition was not met in time");
    await new Promise((done) => setTimeout(done, 5));
  }
}

function executionResult(cwd: string | undefined): ExecutionResult {
  const now = new Date();
  return {
    status: "completed",
    exitCode: 0,
    stdout: { text: cwd ?? "", bytes: Buffer.byteLength(cwd ?? ""), truncated: false },
    stderr: { text: "", bytes: 0, truncated: false },
    durationMs: 1,
    timedOut: false,
    cancelled: false,
    startedAt: now,
    completedAt: now,
  };
}

function fakeWorktreePort(
  options: { ensureFails?: boolean; jj?: boolean; onCreate?: (path: string) => void } = {},
) {
  const events: string[] = [];
  const port: WorktreeIsolationPort = {
    async create({ cwd, agentId }) {
      events.push(`create:${agentId}`);
      options.onCreate?.(join(cwd, ".zcode", "worktrees", agentId));
      return {
        ...(options.jj
          ? {
              vcs: "jj" as const,
              env: { GIT_CEILING_DIRECTORIES: join(cwd, ".zcode", "worktrees") },
            }
          : {}),
        repoRoot: cwd,
        path: join(cwd, ".zcode", "worktrees", agentId),
        branch: `zcode/${agentId}`,
        sourceHead: "a".repeat(40),
        cargoTargetDir: join(cwd, "target"),
      };
    },
    async ensure(record) {
      events.push("ensure");
      if (options.ensureFails) {
        throw new WorktreeIsolationError(
          "unavailable",
          `isolation unavailable: the worktree ${record.path} is missing and could not be recreated`,
        );
      }
      const { removed: _removed, ...rest } = record;
      return rest;
    },
    async finalize(record, { message }): Promise<WorktreeFinalizeResult> {
      events.push(`finalize:${message}`);
      return {
        record: { ...record, resultHead: RESULT_HEAD },
        outcome: "committed",
        commitsAhead: 1,
        diffStat: "1 file changed, 1 insertion(+)",
      };
    },
  };
  return { events, port };
}

async function setup(input: {
  steps: ModelStep[];
  worktree?: WorktreeIsolationPort;
  executionPort?: ExecutionPort;
  profileIsolation?: boolean;
  fileSystemPort?: MemoryFileSystem;
  codeSearchPort?: CodeSearchPort;
  scopePortsFor?: (scope: { root: string; env?: Record<string, string> }) => {
    codeSearchPort?: CodeSearchPort;
  };
}) {
  const outputRootDir = await mkdtemp(join(tmpdir(), "zcode-subagent-worktree-"));
  const sessionId = createSessionId(`worktree-${Math.random().toString(36).slice(2)}`);
  const requests: ExecutionRequest[] = [];
  const executionPort: ExecutionPort = input.executionPort ?? {
    async run(request) {
      requests.push(request);
      return executionResult(request.cwd);
    },
  };
  const sessionStore: SessionStorePort = createRecordingSubagentSessionStore();
  const fileSystemPort =
    input.fileSystemPort ??
    new MemoryFileSystem({ [join(workspace, "a.txt")]: "MAIN_CHECKOUT_CONTENT\n" });
  let step = 0;
  const modelTexts: string[] = [];
  const runtime = createTestAgentRuntime(
    sessionId,
    {
      mode: "yolo",
      workingDirectory: workspace,
      subagents: {
        outputRootDir,
        profiles: [
          {
            name: "writer",
            description: "Writes code",
            source: "user",
            systemPrompt: "Write",
            ...(input.profileIsolation ? { isolation: "worktree" as const } : {}),
          },
        ],
      },
    },
    {
      eventStore: createTestSessionEventStore(),
      sessionStore,
      executionPort,
      fileSystemPort,
      ...(input.worktree ? { worktreeIsolationPort: input.worktree } : {}),
      ...(input.codeSearchPort ? { codeSearchPort: input.codeSearchPort } : {}),
      ...(input.scopePortsFor ? { scopePortsFor: input.scopePortsFor } : {}),
      modelFactory: createTestModelFactory({
        async generateText(
          request: { messages: unknown },
          observation?: { invocationContext?: { traceContext?: { sessionId?: string } } },
        ) {
          // 停止通知会唤醒父 runtime 自己跑一轮；脚本步骤只给 child。
          if (observation?.invocationContext?.traceContext?.sessionId === sessionId) {
            return {
              finishReason: "stop",
              text: "parent noted",
              usage: { inputTokens: 1, outputTokens: 1, totalTokens: 2 },
            };
          }
          const text = JSON.stringify(request.messages);
          modelTexts.push(text);
          const current = input.steps[Math.min(step, input.steps.length - 1)]!;
          step += 1;
          const reply = await current(text);
          return reply.toolCalls
            ? {
                finishReason: "tool-calls",
                text: "",
                toolCalls: reply.toolCalls,
                usage: { inputTokens: 1, outputTokens: 1, totalTokens: 2 },
              }
            : {
                finishReason: "stop",
                text: reply.text ?? "child done",
                usage: { inputTokens: 1, outputTokens: 1, totalTokens: 2 },
              };
        },
      } as never),
    },
  );
  const internal = runtime as any;
  const turnId = createTurnId();
  const context = {
    sessionId,
    turnId,
    trace: { traceId: createTraceId(), sessionId, turnId },
    workingDirectory: workspace,
    workspaceRoot: workspace,
  };
  return {
    context,
    fileSystemPort,
    internal,
    modelTexts,
    outputRootDir,
    port: internal.subagentPort,
    requests,
    sessionId,
    sessionStore,
  };
}

function runningAgentId(internal: any): string | undefined {
  return Object.values(internal.runtimeTaskRegistry.all()).find(
    (task: any) => task.type === "local_agent" && task.status === "running",
  )?.agentId as string | undefined;
}

describe("subagent worktree isolation", () => {
  it("runs the child in its worktree and finalizes after it completes", async () => {
    const fileSystemPort = new MemoryFileSystem({
      [join(workspace, "a.txt")]: "MAIN_CHECKOUT_CONTENT\n",
    });
    const worktree = fakeWorktreePort({
      onCreate: (path) => {
        fileSystemPort.files[join(path, "a.txt")] = "WORKTREE_CONTENT\n";
      },
    });
    const searched: string[] = [];
    const codeSearch = (label: string): CodeSearchPort => ({
      async findFiles() {
        searched.push(label);
        return { ok: true, files: [{ path: `${label}.rs` }], totalMatched: 1 };
      },
      async searchCode() {
        searched.push(label);
        return { ok: true, matches: [], mode: "plain", fuzzyFallback: false };
      },
    });
    const scopes: Array<{ root: string; env?: Record<string, string> }> = [];
    const env = await setup({
      worktree: worktree.port,
      fileSystemPort,
      codeSearchPort: codeSearch("parent_index"),
      scopePortsFor: (scope) => {
        scopes.push(scope);
        return { codeSearchPort: codeSearch("worktree_index") };
      },
      steps: [
        () => ({
          toolCalls: [
            { id: "pwd", name: "Bash", input: { command: "pwd", description: "where" } },
            { id: "read", name: "Read", input: { file_path: "a.txt" } },
            { id: "find", name: "FindFiles", input: { query: "lib" } },
          ],
        }),
        () => ({ text: "child done" }),
      ],
    });
    const output = await env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Edit things",
      isolation: "worktree",
      parentToolCallId: "call_1",
      prompt: "CHILD_TASK",
    });
    const worktreePath = join(workspace, ".zcode", "worktrees", output.agentId);

    expect(env.requests).toHaveLength(1);
    expect(env.requests[0]?.cwd).toBe(worktreePath);
    expect(env.requests[0]?.env?.set?.CARGO_TARGET_DIR).toBe(join(workspace, "target"));
    expect(env.modelTexts[0]).toContain("CHILD_TASK");
    expect(env.modelTexts[0]).toContain("worktree_isolation");
    expect(env.modelTexts[1]).toContain("WORKTREE_CONTENT");
    expect(env.modelTexts[1]).not.toContain("MAIN_CHECKOUT_CONTENT");
    expect(scopes).toEqual([
      { root: worktreePath, env: { CARGO_TARGET_DIR: join(workspace, "target") } },
    ]);
    expect(searched).toEqual(["worktree_index"]);
    expect(worktree.events).toEqual([
      `create:${output.agentId}`,
      `finalize:zcode agent ${output.agentId}: Edit things`,
    ]);
    const text = output.content[0].text as string;
    expect(text).toContain("child done");
    expect(text).toContain(`git merge --no-ff zcode/${output.agentId}`);
    expect(text).toContain("1 file changed");

    const task = await env.port.getTask(output.agentId);
    expect(task.worktree).toMatchObject({ path: worktreePath, resultHead: RESULT_HEAD });
    const metadata = JSON.parse(
      await readFile(task.outputFile.replace("output.txt", "metadata.json"), "utf8"),
    );
    expect(metadata.worktree).toMatchObject({ path: worktreePath, resultHead: RESULT_HEAD });
    // session 归属父工作区；执行目录只在 worktree。
    const child = await env.sessionStore.getSession(task.childSessionId);
    expect(child?.directory).toBe(workspace);

    const restored = await readAgentMetadataSnapshot(
      env.outputRootDir,
      env.sessionId,
      output.agentId,
    );
    expect(restored?.worktree).toMatchObject({ branch: `zcode/${output.agentId}` });
    const listed = await listAgentsToolEntry.handler({}, {
      sessionId: env.sessionId,
      subagentPort: env.port,
    } as never);
    expect((listed as any).agents[0].worktree).toEqual({
      branch: `zcode/${output.agentId}`,
      path: worktreePath,
    });
  });

  it("gives a jj workspace child its backend env and jj wording", async () => {
    const worktree = fakeWorktreePort({ jj: true });
    const scopes: Array<{ root: string; env?: Record<string, string> }> = [];
    const env = await setup({
      worktree: worktree.port,
      scopePortsFor: (scope) => {
        scopes.push(scope);
        return {};
      },
      steps: [
        () => ({
          toolCalls: [{ id: "pwd", name: "Bash", input: { command: "pwd", description: "where" } }],
        }),
        () => ({ text: "child done" }),
      ],
    });
    const output = await env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Edit things",
      isolation: "worktree",
      parentToolCallId: "call_1",
      prompt: "CHILD_TASK",
    });
    const expectedEnv = {
      GIT_CEILING_DIRECTORIES: join(workspace, ".zcode", "worktrees"),
      CARGO_TARGET_DIR: join(workspace, "target"),
    };

    expect(env.requests[0]?.env?.set).toMatchObject(expectedEnv);
    expect(scopes[0]?.env).toEqual(expectedEnv);
    expect(env.modelTexts[0]).toContain("isolated jj workspace");
    expect(env.modelTexts[0]).toContain("Use jj here, not git");
    const text = output.content[0].text as string;
    expect(text).toContain(`set bookmark zcode/${output.agentId}`);
    expect(text).toContain(
      `jj rebase -b zcode/${output.agentId} -d @- && jj rebase -s @ -d zcode/${output.agentId}`,
    );
    expect(text).not.toContain("git merge");
  });

  it("does not finalize at the background handoff, only when the child finishes", async () => {
    const worktree = fakeWorktreePort();
    const gate = deferred();
    const env = await setup({
      worktree: worktree.port,
      steps: [
        async () => {
          await gate.promise;
          return { text: "late result" };
        },
      ],
    });
    const running = env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Slow",
      isolation: "worktree",
      parentToolCallId: "call_bg",
      prompt: "SLOW",
    });
    await waitFor(() => env.modelTexts.length > 0);
    const agentId = runningAgentId(env.internal)!;
    await env.port.backgroundTask(agentId);
    const handed = await running;
    expect(handed.status).toBe("async_launched");
    expect(worktree.events.filter((event) => event.startsWith("finalize"))).toEqual([]);

    gate.resolve();
    const done = await env.port.waitForTask(agentId);
    expect(done.status).toBe("completed");
    expect(worktree.events.filter((event) => event.startsWith("finalize"))).toHaveLength(1);
    expect(done.output.content[0].text).toContain("<worktree_result>");
  });

  it("stops background commands the child left running before finalizing", async () => {
    const worktree = fakeWorktreePort();
    const startedAt = new Date(0);
    const order: string[] = [];
    const executionPort = {
      async run() {
        throw new Error("foreground run is not used");
      },
      async runBashWithBackgroundLifecycle(request: ExecutionRequest) {
        order.push(`start:${request.cwd}`);
        return {
          kind: "backgrounded",
          task: { taskId: "writer_bg", status: "running", startedAt },
        };
      },
      async getBackgroundTask(taskId: string) {
        return { taskId, status: "running", startedAt };
      },
      async cancelBackgroundTask(taskId: string) {
        order.push(`cancel:${taskId}`);
        return { taskId, status: "cancelled", startedAt, completedAt: new Date(1) };
      },
    } as unknown as ExecutionPort;
    const finalize = worktree.port.finalize;
    worktree.port.finalize = async (record, input) => {
      order.push("finalize");
      return finalize(record, input);
    };
    const env = await setup({
      worktree: worktree.port,
      executionPort,
      steps: [
        () => ({
          toolCalls: [
            {
              id: "bg",
              name: "Bash",
              input: { command: "node watch.js", description: "watch", run_in_background: true },
            },
          ],
        }),
        () => ({ text: "left it running" }),
      ],
    });
    const output = await env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Watcher",
      isolation: "worktree",
      parentToolCallId: "call_w",
      prompt: "WATCH",
    });
    const worktreePath = join(workspace, ".zcode", "worktrees", output.agentId);
    expect(order).toEqual([`start:${worktreePath}`, "cancel:writer_bg", "finalize"]);
    expect(output.content[0].text).toContain("Stopped 1 background command");
  });

  it("keeps worktree metadata through StopTask and resumes cold inside the worktree", async () => {
    const worktree = fakeWorktreePort();
    const gate = deferred();
    const env = await setup({
      worktree: worktree.port,
      steps: [
        async () => {
          await gate.promise;
          return { text: "stopped turn" };
        },
        () => ({
          toolCalls: [
            { id: "pwd2", name: "Bash", input: { command: "pwd", description: "where" } },
          ],
        }),
        () => ({ text: "resumed done" }),
      ],
    });
    const started = await env.port.start({
      ...env.context,
      agentType: "writer",
      description: "Stoppable",
      isolation: "worktree",
      parentToolCallId: "call_s",
      prompt: "STOPPABLE",
    });
    const worktreePath = join(workspace, ".zcode", "worktrees", started.agentId);
    await waitFor(() => env.modelTexts.length > 0);
    const stopped = await env.port.stopTask(started.agentId);
    expect(stopped.worktree).toMatchObject({ path: worktreePath });
    gate.resolve();
    await waitFor(() => worktree.events.some((event) => event.startsWith("finalize")));
    const metadataFile = join(env.outputRootDir, env.sessionId, started.agentId, "metadata.json");
    await waitFor(async () => {
      const metadata = JSON.parse(await readFile(metadataFile, "utf8"));
      return metadata.worktree?.resultHead === RESULT_HEAD;
    });
    const metadata = JSON.parse(await readFile(metadataFile, "utf8"));
    expect(metadata.status).toBe("stopped");

    // 冷恢复：registry 丢失，只剩 metadata.json。
    env.internal.runtimeTaskRegistry.remove(started.agentId);
    const resumed = await env.port.sendMessage({
      ...env.context,
      to: started.agentId,
      message: "CONTINUE",
      summary: "Continue",
      parentToolCallId: "call_r",
    });
    expect(resumed).toMatchObject({ status: "success", delivery: "resumed_background" });
    const done = await env.port.waitForTask(started.agentId);
    expect(done.status).toBe("completed");
    expect(worktree.events).toContain("ensure");
    expect(env.requests.map((request) => request.cwd)).toEqual([worktreePath]);
  });

  it("refuses to resume in the parent checkout when the worktree cannot be recreated", async () => {
    const worktree = fakeWorktreePort({ ensureFails: true });
    const env = await setup({ worktree: worktree.port, steps: [() => ({ text: "first" })] });
    const first = await env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Once",
      isolation: "worktree",
      parentToolCallId: "call_o",
      prompt: "ONCE",
    });
    const calls = env.modelTexts.length;
    const result = await env.port.sendMessage({
      ...env.context,
      to: first.agentId,
      message: "AGAIN",
      summary: "Again",
      parentToolCallId: "call_a",
    });
    expect(result.status).toBe("failed");
    expect(result.error).toContain("isolation unavailable");
    expect(env.modelTexts).toHaveLength(calls);
    expect((await env.port.getTask(first.agentId)).status).toBe("completed");
  });

  it("errors before launch when worktree isolation is unavailable", async () => {
    const env = await setup({ steps: [() => ({ text: "never" })] });
    await expect(
      env.port.run({
        ...env.context,
        agentType: "writer",
        description: "No port",
        isolation: "worktree",
        parentToolCallId: "call_n",
        prompt: "NOPE",
      }),
    ).rejects.toThrow(/not available in this workspace/u);
    expect(env.modelTexts).toHaveLength(0);
    expect(runningAgentId(env.internal)).toBeUndefined();
  });

  it("applies an agent file's isolation default", async () => {
    const worktree = fakeWorktreePort();
    const env = await setup({
      worktree: worktree.port,
      profileIsolation: true,
      steps: [() => ({ text: "ok" })],
    });
    const output = await env.port.run({
      ...env.context,
      agentType: "writer",
      description: "Default",
      parentToolCallId: "call_d",
      prompt: "DEFAULT",
    });
    expect(worktree.events[0]).toBe(`create:${output.agentId}`);

    const parsed = parseAgentProfileFromMarkdown({
      content: "---\nname: isolated\ndescription: d\nisolation: worktree\n---\nBody",
      source: "user",
    });
    expect(parsed.profile?.isolation).toBe("worktree");
  });
});

export type { WorktreeRecord };
