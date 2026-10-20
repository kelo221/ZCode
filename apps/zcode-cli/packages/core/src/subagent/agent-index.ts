// ============================================================
// Subagent metadata index
// ============================================================
// registry 只活在当前 runtime 的内存里；CLI 重启、runtime 替换或宿主空闲卸载后，模型手里的
// agent id 就再也解析不到。每个 subagent 在启动前已把 metadata.json 写进
// `<outputRoot>/<parentSessionId>/<agentId>/`，这里把它读回来：SendMessage 用它重建 terminal
// snapshot 以便 resume，ListAgents 与 compact 后的 reminder 用它列出本 session 的 subagent。

import {
  WorktreeRecordSchema,
  type SessionId,
  type SubagentListEntry,
  type SubagentTaskStatus,
} from "@zcode/contracts";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import type { RuntimeTaskRegistry, RuntimeTaskSnapshot } from "../runtime-task/registry.js";

/** agent id 会拼进文件路径；只放行 runner 生成的形状，挡住 `..`、分隔符等路径逃逸。 */
const AGENT_ID_PATTERN = /^agent_[A-Za-z0-9-]{1,100}$/;

export function isWellFormedAgentId(value: string): boolean {
  return AGENT_ID_PATTERN.test(value);
}

type SubagentIndexEntry = SubagentListEntry;

interface AgentMetadataFile {
  agentId: string;
  childSessionId: string;
  createdAt?: string;
  description?: string;
  outputFile?: string;
  parentSessionId: string;
  parentToolUseId?: string;
  profileId: string;
  prompt?: string;
  status?: string;
  updatedAt?: string;
  worktree?: unknown;
}

/**
 * 读一个 agent 的 metadata 并转成可直接注册进 registry 的 terminal snapshot。
 * 不属于 `sessionId` 的 agent、格式不对的 id、缺失或损坏的文件都返回 undefined。
 */
export async function readAgentMetadataSnapshot(
  outputRootDir: string,
  sessionId: SessionId,
  agentId: string,
): Promise<RuntimeTaskSnapshot | undefined> {
  if (!isWellFormedAgentId(agentId)) return undefined;
  const metadata = await readAgentMetadataFile(join(outputRootDir, sessionId, agentId));
  if (!metadata || metadata.agentId !== agentId || metadata.parentSessionId !== sessionId) {
    return undefined;
  }
  return snapshotFromMetadata(metadata);
}

/** 当前 session 的全部 subagent：registry 条目优先，其余从磁盘 metadata 补齐；最近更新的在前。 */
export async function listSessionSubagents(input: {
  outputRootDir: string;
  registry: RuntimeTaskRegistry;
  sessionId: SessionId;
}): Promise<SubagentIndexEntry[]> {
  const diskEntries = (await readSessionMetadataFiles(input.outputRootDir, input.sessionId)).map(
    (metadata) => {
      const snapshot = snapshotFromMetadata(metadata);
      return {
        ...indexEntryFromSnapshot(snapshot),
        updatedAt: parseDate(metadata.updatedAt) ?? snapshot.startedAt,
      };
    },
  );
  return mergeSessionSubagents(diskEntries, input.registry, input.sessionId);
}

/** 同步合并：registry 中属于 `sessionId` 的 local_agent 覆盖同 id 的已知条目。 */
export function mergeSessionSubagents(
  known: readonly SubagentIndexEntry[],
  registry: RuntimeTaskRegistry,
  sessionId: SessionId,
): SubagentIndexEntry[] {
  const byId = new Map<string, SubagentIndexEntry>();
  for (const entry of known) byId.set(entry.agentId, entry);
  for (const task of Object.values(registry.all())) {
    if (task.type !== "local_agent" || task.parentSessionId !== sessionId) continue;
    byId.set(task.agentId, { ...indexEntryFromSnapshot(task), live: true });
  }
  return [...byId.values()].sort(
    (left, right) => right.updatedAt.getTime() - left.updatedAt.getTime(),
  );
}

/** compact 后附带的 reminder；没有 subagent 时返回 undefined。 */
export function formatSubagentRosterReminder(
  entries: readonly SubagentIndexEntry[],
  limit = 20,
): string | undefined {
  if (entries.length === 0) return undefined;
  const shown = entries.slice(0, limit);
  return [
    "Subagents spawned earlier in this session (most recent first). Their conversations are kept: use SendMessage with to: '<agentId>' to continue one instead of starting a new Agent.",
    ...shown.map(formatSubagentIndexLine),
    ...(entries.length > shown.length
      ? [
          `(${entries.length - shown.length} older subagents not shown; use ListAgents to see them.)`,
        ]
      : []),
  ].join("\n");
}

export function formatSubagentIndexLine(entry: SubagentIndexEntry): string {
  const worktree =
    entry.worktree && entry.worktree.removed !== true
      ? ` · branch ${entry.worktree.branch} (${entry.worktree.path})`
      : "";
  return `- ${entry.agentId} · ${entry.agentType} · ${entry.status} · ${JSON.stringify(entry.description)} · updated ${entry.updatedAt.toISOString()}${worktree}`;
}

function indexEntryFromSnapshot(task: RuntimeTaskSnapshot): SubagentIndexEntry {
  return {
    agentId: task.agentId,
    agentType: task.agentType,
    description: task.description,
    status: task.status,
    startedAt: task.startedAt,
    updatedAt: task.completedAt ?? task.startedAt,
    live: false,
    ...(task.worktree ? { worktree: task.worktree } : {}),
  };
}

function snapshotFromMetadata(metadata: AgentMetadataFile): RuntimeTaskSnapshot {
  const startedAt = parseDate(metadata.createdAt) ?? new Date(0);
  const updatedAt = parseDate(metadata.updatedAt);
  const worktree = WorktreeRecordSchema.safeParse(metadata.worktree);
  return {
    taskId: metadata.agentId,
    agentId: metadata.agentId,
    agentType: metadata.profileId,
    childSessionId: metadata.childSessionId as SessionId,
    description: metadata.description ?? "",
    isBackgrounded: true,
    ...(metadata.outputFile ? { outputFile: metadata.outputFile } : {}),
    ...(metadata.parentToolUseId ? { parentToolCallId: metadata.parentToolUseId } : {}),
    parentSessionId: metadata.parentSessionId as SessionId,
    ...(metadata.prompt ? { prompt: metadata.prompt } : {}),
    startedAt,
    ...(updatedAt ? { completedAt: updatedAt } : {}),
    status: terminalStatusFromMetadata(metadata.status),
    // 磁盘上的终态早已通知过（或随进程一起丢了）；不能让重建再触发一次 task-notification。
    notified: true,
    taskType: "local_agent",
    type: "local_agent",
    ...(worktree.success ? { worktree: worktree.data } : {}),
  };
}

/** 重建时持有它的进程已不在，所以 `running` 只可能是被打断的 agent，按 `stopped` 处理。 */
function terminalStatusFromMetadata(status: string | undefined): SubagentTaskStatus {
  if (status === "completed" || status === "failed") return status;
  return "stopped";
}

async function readSessionMetadataFiles(
  outputRootDir: string,
  sessionId: SessionId,
): Promise<AgentMetadataFile[]> {
  let names: string[];
  try {
    names = await readdir(join(outputRootDir, sessionId));
  } catch {
    return [];
  }
  const files = await Promise.all(
    names
      .filter(isWellFormedAgentId)
      .map((name) => readAgentMetadataFile(join(outputRootDir, sessionId, name))),
  );
  return files.filter(
    (metadata): metadata is AgentMetadataFile =>
      metadata !== undefined && metadata.parentSessionId === sessionId,
  );
}

async function readAgentMetadataFile(agentDir: string): Promise<AgentMetadataFile | undefined> {
  let raw: string;
  try {
    raw = await readFile(join(agentDir, "metadata.json"), "utf8");
  } catch {
    return undefined;
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return isAgentMetadataFile(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
}

function isAgentMetadataFile(value: unknown): value is AgentMetadataFile {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.agentId === "string" &&
    isWellFormedAgentId(record.agentId) &&
    typeof record.childSessionId === "string" &&
    record.childSessionId.length > 0 &&
    typeof record.parentSessionId === "string" &&
    typeof record.profileId === "string" &&
    record.profileId.length > 0
  );
}

function parseDate(value: string | undefined): Date | undefined {
  if (value === undefined) return undefined;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date;
}
