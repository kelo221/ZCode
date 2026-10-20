// ============================================================
// Subagent worktree isolation（Fork）
// ============================================================
// 合同见 docs/design/v2/tool/07-subagent.md「Worktree isolation 当前范围」。
// runner 只通过这里使用 WorktreeIsolationPort：创建、恢复前重建、在 child 真正结束后收尾。

import {
  AgentErrorCode,
  CoreErrorType,
  WorktreeIsolationError,
  createCoreError,
  type ExecutionPort,
  type ExecutionRequest,
  type WorktreeFinalizeResult,
  type WorktreeIsolationPort,
  type WorktreeRecord,
} from "@zcode/contracts";
import { readFile, writeFile } from "node:fs/promises";
import type { AgentProfile } from "./profile.js";

export interface SubagentIsolationState {
  record: WorktreeRecord;
  /** 最近一次收尾的模型可见摘要；收尾前为 undefined。 */
  summary?: string;
}

export function wantsWorktreeIsolation(
  request: { isolation?: "worktree" },
  profile: Pick<AgentProfile, "isolation">,
): boolean {
  return request.isolation === "worktree" || profile.isolation === "worktree";
}

export async function createSubagentWorktree(
  port: WorktreeIsolationPort | undefined,
  input: { cwd: string; agentId: string; agentType: string },
): Promise<SubagentIsolationState> {
  if (!port) {
    throw isolationError(
      'isolation: "worktree" is not available in this workspace (it needs a local git checkout). Run the agent without isolation.',
      input.agentType,
    );
  }
  try {
    return { record: await port.create({ cwd: input.cwd, agentId: input.agentId }) };
  } catch (error) {
    throw isolationError(errorText(error), input.agentType, error);
  }
}

/**
 * 恢复前确保 worktree 存在：目录缺失时从保留的 branch（再退到 sourceHead）重建。
 * 失败只返回错误文本，调用方报告 "isolation unavailable"，绝不退回父 checkout。
 */
export async function ensureSubagentWorktree(
  port: WorktreeIsolationPort | undefined,
  record: WorktreeRecord,
): Promise<{ state: SubagentIsolationState } | { error: string }> {
  if (!port) {
    return {
      error: `isolation unavailable: this agent ran in worktree ${record.path}, and worktree isolation is not available in this workspace.`,
    };
  }
  await settledIsolationRun(record.path);
  try {
    return { state: { record: await port.ensure(record) } };
  } catch (error) {
    const text = errorText(error);
    return {
      error: text.startsWith("isolation unavailable") ? text : `isolation unavailable: ${text}`,
    };
  }
}

const runsInFlight = new Map<string, Promise<void>>();

/**
 * 同一 worktree 的收尾与下一次恢复串行：停止后立刻 resume 时，旧 run 的收尾
 * （可能删掉无改动的 worktree）必须先结束，ensure 才能看到真实状态。
 */
export function trackIsolationRun(path: string): () => void {
  let release!: () => void;
  const settled = new Promise<void>((resolve) => {
    release = resolve;
  });
  const previous = runsInFlight.get(path) ?? Promise.resolve();
  const chained = previous.then(() => settled);
  runsInFlight.set(path, chained);
  return () => {
    release();
    void chained.then(() => {
      if (runsInFlight.get(path) === chained) runsInFlight.delete(path);
    });
  };
}

async function settledIsolationRun(path: string): Promise<void> {
  await runsInFlight.get(path);
}

export async function finalizeSubagentWorktree(
  port: WorktreeIsolationPort | undefined,
  state: SubagentIsolationState,
  input: { agentId: string; description: string; backgroundTasksStopped: number },
): Promise<SubagentIsolationState> {
  let result: WorktreeFinalizeResult;
  if (!port) {
    result = {
      record: { ...state.record, finalizeError: "worktree isolation port is unavailable" },
      outcome: "kept",
      commitsAhead: 0,
      error: "worktree isolation port is unavailable",
    };
  } else {
    try {
      result = await port.finalize(state.record, {
        message: `zcode agent ${input.agentId}: ${singleLine(input.description)}`,
      });
    } catch (error) {
      const text = errorText(error);
      result = {
        record: { ...state.record, finalizeError: text.slice(0, 2_000) },
        outcome: "kept",
        commitsAhead: 0,
        error: text,
      };
    }
  }
  state.record = result.record;
  state.summary = formatWorktreeSummary(result, input.backgroundTasksStopped);
  return state;
}

export function formatWorktreeSummary(
  result: WorktreeFinalizeResult,
  backgroundTasksStopped = 0,
): string {
  const { record } = result;
  const lines = ["<worktree_result>"];
  if (backgroundTasksStopped > 0) {
    lines.push(
      `Stopped ${backgroundTasksStopped} background command${backgroundTasksStopped === 1 ? "" : "s"} the agent left running before finalizing its worktree.`,
    );
  }
  const stat = result.diffStat ? ` (${result.diffStat})` : "";
  const merge = `Review it, then merge from your checkout with: git merge --no-ff ${record.branch}`;
  if (result.error) {
    lines.push(
      `Finalizing the worktree failed, so its files and branch were kept as they are: ${result.error}`,
      `Worktree: ${record.path} (branch ${record.branch}). Commit or clean up there before merging.`,
    );
  } else if (result.outcome === "removed") {
    lines.push(`The agent made no changes; removed its worktree and branch ${record.branch}.`);
  } else if (result.outcome === "committed") {
    lines.push(
      `Committed the agent's changes to branch ${record.branch}${stat}; ${result.commitsAhead} commit${result.commitsAhead === 1 ? "" : "s"} ahead of ${short(record.sourceHead)}.`,
      `Worktree: ${record.path}`,
      merge,
    );
  } else {
    lines.push(
      `Branch ${record.branch} has ${result.commitsAhead} commit${result.commitsAhead === 1 ? "" : "s"} from the agent${stat}, based on ${short(record.sourceHead)}.`,
      `Worktree: ${record.path}`,
      merge,
    );
  }
  lines.push("</worktree_result>");
  return lines.join("\n");
}

export function formatWorktreePromptNote(record: WorktreeRecord): string {
  return [
    "<worktree_isolation>",
    `You are working in an isolated git worktree at ${record.path} on branch ${record.branch}, created from commit ${short(record.sourceHead)}.`,
    "The parent's uncommitted changes and untracked files are not in this worktree. Your changes are committed to this branch when you finish; the parent merges them.",
    "Stay inside this worktree; do not edit files in the main checkout.",
    "</worktree_isolation>",
  ].join("\n");
}

/** 停止路径先写了 metadata；收尾晚于它完成，只回填 worktree 字段。 */
export async function patchMetadataWorktree(
  metadataFile: string,
  record: WorktreeRecord,
): Promise<void> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(await readFile(metadataFile, "utf8"));
  } catch {
    return;
  }
  if (typeof parsed !== "object" || parsed === null) return;
  await writeFile(
    metadataFile,
    `${JSON.stringify({ ...parsed, worktree: record, updatedAt: new Date().toISOString() }, null, 2)}\n`,
    "utf8",
  );
}

const ENV_REQUEST_METHODS = new Set<PropertyKey>([
  "run",
  "start",
  "runBashWithBackgroundLifecycle",
]);

/**
 * 给 child 的每个执行请求（前台、后台 Bash）叠加环境变量；请求自己显式设置的值优先。
 * 其余方法（等待、读取输出、取消）原样转发给共享的父端口。
 */
export function withExecutionEnv(
  port: ExecutionPort | undefined,
  set: Record<string, string> | undefined,
): ExecutionPort | undefined {
  if (!port || !set || Object.keys(set).length === 0) return port;
  const merge = (request: ExecutionRequest): ExecutionRequest => {
    const unset = new Set(request.env?.unset ?? []);
    const inherited = Object.fromEntries(Object.entries(set).filter(([key]) => !unset.has(key)));
    return { ...request, env: { ...request.env, set: { ...inherited, ...request.env?.set } } };
  };
  return new Proxy(port, {
    get(target, property) {
      const value: unknown = Reflect.get(target, property, target);
      if (typeof value !== "function") return value;
      if (ENV_REQUEST_METHODS.has(property)) {
        return (request: ExecutionRequest, ...rest: unknown[]) =>
          (value as (...args: unknown[]) => unknown).call(target, merge(request), ...rest);
      }
      return (value as (...args: unknown[]) => unknown).bind(target);
    },
  });
}

function isolationError(message: string, agentType: string, cause?: unknown) {
  return createCoreError(CoreErrorType.ToolExecutionFailed, message, {
    ...(cause instanceof Error ? { cause } : {}),
    context: {
      code: AgentErrorCode.CHILD_RUNTIME_FAILED,
      agentType,
      ...(cause instanceof WorktreeIsolationError ? { isolationCode: cause.code } : {}),
    },
    recoverable: true,
  });
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function singleLine(text: string): string {
  return text.replace(/\s+/gu, " ").trim().slice(0, 120) || "work";
}

function short(sha: string): string {
  return sha.slice(0, 10);
}
