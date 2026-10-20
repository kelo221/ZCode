// ============================================================
// ListAgents Tool Handler
// ============================================================
// agent id 只出现在 Agent 结果与 task-notification 里；被 compact 摘要掉、或前台 agent 中途失败时，
// 父模型就找不回它。这个工具从 subagent port 列出当前 session 的 subagent（含 runtime 重启后只剩
// 磁盘 metadata 的），让模型能用 SendMessage 续上原对话，而不是重开一个新 agent。

import {
  CoreErrorType,
  LIST_AGENTS_TOOL_NAME,
  ListAgentsInputJsonSchema,
  ListAgentsInputSchema,
  ListAgentsOutputJsonSchema,
  ListAgentsOutputSchema,
  createCoreError,
  type ListAgentsInput,
  type ListAgentsOutput,
  type ModelMessageContent,
} from "@zcode/contracts";
import type { ToolEntry, ToolHandler } from "../types.js";

const LIST_AGENTS_TIMEOUT_MS = 10_000;
const LIST_AGENTS_MODEL_BYTES = 24_000;

const LIST_AGENTS_DESCRIPTION = [
  "Lists the subagents spawned in this session, most recently updated first, with their agent IDs.",
  "",
  "- Use it when you need an agent ID you no longer have: the Agent result was summarized away, a subagent failed or was interrupted midway, or the app restarted.",
  "- Every listed agent keeps its conversation. Pass its `agentId` to SendMessage to continue it (stopped, failed and completed agents resume in the background) instead of starting a new Agent from scratch.",
  '- `live="false"` means the agent is not loaded in this process (for example after a restart); SendMessage still resumes it from its saved conversation.',
].join("\n");

const listAgentsHandler: ToolHandler = async (input, context) => {
  const parsed = ListAgentsInputSchema.parse(input) as ListAgentsInput;
  const listAgents = context.subagentPort?.listAgents;
  if (!context.subagentPort || typeof listAgents !== "function") {
    throw createCoreError(
      CoreErrorType.ConfigurationError,
      "Subagent port is not configured for ListAgents",
      {
        context: { toolCallId: context.toolCallId, toolName: LIST_AGENTS_TOOL_NAME },
        recoverable: false,
      },
    );
  }

  const entries = await listAgents.call(context.subagentPort, context.sessionId);
  const statuses = parsed.statuses ? new Set<string>(parsed.statuses) : undefined;
  const filtered = statuses ? entries.filter((entry) => statuses.has(entry.status)) : entries;
  return {
    agents: filtered.slice(0, parsed.limit).map((entry) => ({
      agentId: entry.agentId,
      agentType: entry.agentType,
      description: entry.description,
      status: entry.status,
      startedAt: entry.startedAt.toISOString(),
      updatedAt: entry.updatedAt.toISOString(),
      live: entry.live,
      ...(entry.worktree && entry.worktree.removed !== true
        ? { worktree: { branch: entry.worktree.branch, path: entry.worktree.path } }
        : {}),
    })),
    total: filtered.length,
  } satisfies ListAgentsOutput;
};

function formatListAgentsModelContent(output: unknown): ModelMessageContent {
  const parsed = ListAgentsOutputSchema.safeParse(output);
  if (!parsed.success) return "ListAgents returned an invalid result.";
  const { agents, total } = parsed.data;
  const header = `count="${agents.length}" total="${total}"`;
  if (agents.length === 0) {
    return `<agents ${header}>\nNo subagents match in this session.\n</agents>`;
  }
  const rows = agents.map(
    (agent) =>
      `<agent id="${agent.agentId}" type=${JSON.stringify(agent.agentType)} status="${agent.status}" live="${agent.live}" description=${JSON.stringify(agent.description)} started_at="${agent.startedAt}" updated_at="${agent.updatedAt}"${agent.worktree ? ` branch=${JSON.stringify(agent.worktree.branch)} worktree=${JSON.stringify(agent.worktree.path)}` : ""} />`,
  );
  return [`<agents ${header}>`, ...rows, "</agents>"].join("\n");
}

export const listAgentsToolEntry: ToolEntry = {
  capability: "List this session's subagents and their agent IDs",
  metadata: {
    shouldDefer: true,
    name: LIST_AGENTS_TOOL_NAME,
    searchHint: "find subagent agent ids to resume with SendMessage",
    description: LIST_AGENTS_DESCRIPTION,
    readOnly: true,
    destructive: false,
    concurrentSafe: true,
    timeoutMs: LIST_AGENTS_TIMEOUT_MS,
    maxOutputBytes: LIST_AGENTS_MODEL_BYTES,
    sideEffectScope: "none",
    riskLevel: "low",
    needsApproval: false,
  },
  handler: listAgentsHandler,
  inputSchema: ListAgentsInputJsonSchema,
  outputSchema: ListAgentsOutputJsonSchema,
  runtimeInputSchema: ListAgentsInputSchema,
  runtimeOutputSchema: ListAgentsOutputSchema,
  formatModelContent: formatListAgentsModelContent,
  permission: {
    permission: "listAgents",
    reason: "ListAgents reads this session's subagent records",
    riskLevel: "low",
    sideEffectScope: "none",
    needsApproval: false,
    patternSources: ["toolName"],
    alwaysAllowPatternSources: ["toolName"],
    denyPriority: "beforeAsk",
  },
  resultBudget: {
    maxInlineBytes: LIST_AGENTS_MODEL_BYTES,
    maxModelBytes: LIST_AGENTS_MODEL_BYTES,
    strategy: "truncate",
    preview: {
      maxBytes: LIST_AGENTS_MODEL_BYTES,
      direction: "head",
    },
  },
  timeout: {
    kind: "timed",
    defaultMs: LIST_AGENTS_TIMEOUT_MS,
    maxMs: LIST_AGENTS_TIMEOUT_MS,
    allowCallOverride: false,
  },
  cancellation: {
    supported: false,
    cleanup: "none",
    userVisibleMessage: "ListAgents reads local subagent records and cannot be cancelled",
  },
  trace: {
    required: true,
    propagateToAdapters: false,
    recordInput: "summary",
    recordOutput: "summary",
  },
};
