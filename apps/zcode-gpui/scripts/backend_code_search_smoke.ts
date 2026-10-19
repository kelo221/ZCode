// End-to-end check of the built-in fff code search (PARITY.md "Built-in fff code search").
// A mock model calls FindFiles, then SearchCode, then answers; the run asserts the tools are
// offered only when the host answers codeSearchEnabled, that they return real hits, and that
// the index closes about a minute after the session goes idle.
//
//   bun apps/zcode-gpui/scripts/backend_code_search_smoke.ts [--skip-idle]
//   ZCODE_GPUI_SMOKE_BACKEND=apps/zcode-gpui/target/backend/zcode-backend.exe bun ...
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { commandAckSchema, commandEnvelopeSchema } from "@zcode/shared/zcode-protocol-v4";
import { smokeProviderConfig } from "./backend_mock.js";

const root = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const skipIdle = process.argv.includes("--skip-idle");
const modelSelection = {
  providerId: "smoke-openai",
  modelId: "mock-chat",
  options: { reasoningLevel: "low" },
};

type Body = { tools?: Array<{ function?: { name?: string } }>; messages?: unknown[] };

async function mockModel() {
  const bodies: Body[] = [];
  const script = [
    { name: "FindFiles", args: { query: "needle_file" } },
    { name: "SearchCode", args: { query: "SMOKE_NEEDLE_42" } },
  ];
  const server = createServer(async (request, response) => {
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    const body = JSON.parse(Buffer.concat(chunks).toString()) as Body;
    bodies.push(body);
    const toolTurns = (body.messages ?? []).filter(
      (m) => (m as { role?: string }).role === "tool",
    ).length;
    const offered = new Set((body.tools ?? []).map((t) => t.function?.name));
    const step = script[toolTurns];
    const delta =
      step && offered.has(step.name)
        ? {
            role: "assistant",
            tool_calls: [
              {
                index: 0,
                id: `call_${toolTurns}`,
                type: "function",
                function: { name: step.name, arguments: JSON.stringify(step.args) },
              },
            ],
          }
        : { role: "assistant", content: "ZCODE_SMOKE_OK" };
    const frames = [
      { id: "m", object: "chat.completion.chunk", choices: [{ index: 0, delta }] },
      {
        id: "m",
        object: "chat.completion.chunk",
        choices: [
          { index: 0, delta: {}, finish_reason: "tool_calls" in delta ? "tool_calls" : "stop" },
        ],
        usage: { prompt_tokens: 10, completion_tokens: 3, total_tokens: 13 },
      },
    ];
    response
      .writeHead(200, { "content-type": "text/event-stream" })
      .end(frames.map((f) => `data: ${JSON.stringify(f)}\n\n`).join("") + "data: [DONE]\n\n");
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("no mock port");
  return { server, port: address.port, bodies };
}

async function run(codeSearchEnabled: boolean) {
  const scratch = await mkdtemp(join(tmpdir(), "zcode-gpui-code-search-"));
  const mock = await mockModel();
  const home = join(scratch, "home");
  const workspace = join(scratch, "workspace");
  const dataDir = join(scratch, "data");
  await mkdir(join(workspace, "src", "deep"), { recursive: true });
  await mkdir(home, { recursive: true });
  await writeFile(join(workspace, "src", "deep", "needle_file.ts"), "export const x = 1;\n");
  await writeFile(join(workspace, "src", "other.ts"), "// SMOKE_NEEDLE_42 lives here\n");
  const providerFile = join(scratch, "provider_config.json");
  const provider = smokeProviderConfig(modelSelection, mock.port);
  // The system prompt plus two tool results overflow the shared 8K mock window and trigger compaction.
  provider.config.modelConfigRules.providerModelRules[0]!.config.properties.contextWindow = 200_000;
  await writeFile(providerFile, JSON.stringify(provider));
  const compiled =
    process.env.ZCODE_GPUI_SMOKE_BACKEND && resolve(process.env.ZCODE_GPUI_SMOKE_BACKEND);
  const serverArgs = ["app-server", "--stdio", "--surface", "desktop"];
  const child = spawn(
    compiled ?? process.execPath,
    compiled ? serverArgs : [join(root, "apps/zcode-cli/packages/cli/src/main.ts"), ...serverArgs],
    {
      cwd: workspace,
      stdio: ["pipe", "pipe", "pipe"],
      env: {
        PATH: process.env.PATH,
        SystemRoot: process.env.SystemRoot,
        WINDIR: process.env.WINDIR,
        TEMP: scratch,
        TMP: scratch,
        HOME: home,
        USERPROFILE: home,
        APPDATA: home,
        LOCALAPPDATA: home,
        ZCODE_RUNTIME_ENV: "desktop",
        ZCODE_DESKTOP_HOME_DIR: home,
        ZCODE_DATA_BASE_DIR: dataDir,
        ZCODE_STORAGE_DIR: join(scratch, "storage"),
        ZCODE_SESSION_DB_PATH: join(scratch, "session.db"),
        ZCODE_BUILTIN_PROVIDER_CONFIG_FILE: join(root, "config/provider/zcode-builtin.json"),
        ZCODE_PERSONAL_PROVIDER_CONFIG_FILE: providerFile,
      },
    },
  );
  let stderr = "";
  child.stderr.on("data", (c) => (stderr = (stderr + c.toString()).slice(-8000)));
  const notifications: Array<{ method?: string; params?: { phase?: string } }> = [];
  const pending = new Map<number, (m: { result?: unknown; error?: unknown }) => void>();
  createInterface({ input: child.stdout }).on("line", (line) => {
    let m: {
      id?: number;
      method?: string;
      params?: { phase?: string };
      result?: unknown;
      error?: unknown;
    };
    try {
      m = JSON.parse(line);
    } catch {
      return;
    }
    if (m.method && m.id !== undefined) {
      const result =
        m.method === "session/requestRuntimePreferences"
          ? {
              result: {
                askUserQuestionAutoResolutionEnabled: true,
                nativeSearchEnhancementsEnabled: true,
                memoryEnabled: false,
                ...(codeSearchEnabled ? { codeSearchEnabled: true } : {}),
              },
            }
          : { error: { code: -32603, message: "not available in smoke" } };
      child.stdin.write(JSON.stringify({ id: m.id, ...result }) + "\n");
    } else if (typeof m.id === "number") pending.get(m.id)?.(m);
    else notifications.push(m);
  });
  let nextId = 0;
  const rpc = (method: string, params: unknown) =>
    new Promise<unknown>((done, fail) => {
      const id = ++nextId;
      const timer = setTimeout(() => fail(new Error(`${method} timed out; ${stderr}`)), 60_000);
      pending.set(id, (m) => {
        clearTimeout(timer);
        if (m.error) fail(new Error(JSON.stringify(m.error)));
        else done(m.result);
      });
      child.stdin.write(JSON.stringify({ id, method, params }) + "\n");
    });
  const command = async (type: string, sessionId: string | null, payload: unknown) =>
    commandAckSchema.parse(
      await rpc(
        "v4/command",
        commandEnvelopeSchema.parse({
          commandId: randomUUID(),
          clientId: "gpui-code-search-smoke",
          sessionId,
          type,
          issuedAt: Date.now(),
          payload,
        }),
      ),
    );
  const waitFor = async (predicate: () => boolean, label: string, timeout = 60_000) => {
    const end = Date.now() + timeout;
    while (!predicate()) {
      if (child.exitCode !== null) throw new Error(`${label}: backend exited; ${stderr}`);
      if (Date.now() > end) throw new Error(`${label}: timed out; ${stderr}`);
      await sleep(50);
    }
  };
  const logEvents = async () => {
    const found: string[] = [];
    const walk = async (dir: string): Promise<void> => {
      for (const entry of await readdir(dir, { withFileTypes: true }).catch(() => [])) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) await walk(path);
        else if (/\.(log|jsonl)$/.test(entry.name)) {
          const text = await readFile(path, "utf8").catch(() => "");
          for (const match of text.matchAll(/zcode_protocol\.code_search\.[a-z_]+/g))
            found.push(match[0]);
        }
      }
    };
    await walk(scratch);
    return found;
  };
  try {
    await waitFor(
      () =>
        notifications.some(
          (m) => m.method === "startup/storageState" && m.params?.phase === "ready",
        ),
      "storage startup",
    );
    const created = await command("createSession", null, {
      workspaceId: workspace,
      config: { modelSelection, mode: "yolo" },
    });
    if (created.status !== "accepted" || created.result?.type !== "createSession")
      throw new Error(`create: ${JSON.stringify(created)}`);
    const sid = created.result.sessionId;
    const sent = await command("sendText", sid, {
      text: "Find the needle",
      requestedDelivery: "startNow",
      modelSelection,
    });
    if (sent.status !== "accepted") throw new Error(`send: ${JSON.stringify(sent)}`);
    const expectedRequests = codeSearchEnabled ? 3 : 1;
    await waitFor(() => mock.bodies.length >= expectedRequests, "model requests");
    await sleep(500);
    const offered = (mock.bodies[0]?.tools ?? []).map((t) => t.function?.name);
    const hasTools = offered.includes("FindFiles") && offered.includes("SearchCode");
    if (hasTools !== codeSearchEnabled)
      throw new Error(
        `tools offered=${hasTools}, expected ${codeSearchEnabled}: ${offered.join(",")}`,
      );
    const result: Record<string, unknown> = { codeSearchEnabled, toolsOffered: hasTools };
    if (codeSearchEnabled) {
      const toolResults = mock.bodies
        .flatMap((body) => body.messages ?? [])
        .filter((m) => (m as { role?: string }).role === "tool")
        .map((m) => String((m as { content?: unknown }).content));
      if (!toolResults.some((text) => text.includes("src/deep/needle_file.ts")))
        throw new Error(`FindFiles result missing the file: ${toolResults.join(" | ")}`);
      if (!toolResults.some((text) => text.includes("src/other.ts\n 1: // SMOKE_NEEDLE_42")))
        throw new Error(`SearchCode result missing the hit: ${toolResults.join(" | ")}`);
      result.findFiles = true;
      result.searchCode = true;
      if (!skipIdle) {
        // Closes 60-65 s after the turn ends (5 s poll); 75 s leaves margin.
        await sleep(75_000);
        if (child.exitCode !== null) throw new Error(`backend exited while idle; ${stderr}`);
        const events = await logEvents();
        result.logEvents = events;
        if (!events.includes("zcode_protocol.code_search.opened"))
          throw new Error(`no open event in logs: ${events.join(",")}`);
        if (!events.includes("zcode_protocol.code_search.idle_closed"))
          throw new Error(`index did not close after idle: ${events.join(",")}`);
        result.idleClosed = true;
      }
    }
    return result;
  } finally {
    child.stdin.end();
    if (child.exitCode === null) {
      child.kill();
      await Promise.race([new Promise((done) => child.once("exit", done)), sleep(3000)]);
    }
    mock.server.close();
    await rm(scratch, { recursive: true, force: true }).catch(() => undefined);
  }
}

const off = await run(false);
const on = await run(true);
console.log(JSON.stringify({ result: "passed", off, on }));
