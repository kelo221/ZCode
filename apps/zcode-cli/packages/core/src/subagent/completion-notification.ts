import {
  SUBAGENT_REMAINING_ITEMS_TAG,
  SessionEventType,
  getModelUsageContextTokens,
  type AgentContextReport,
  type AgentIncompleteReport,
  type ModelUsage,
  type SessionEvent,
} from "@zcode/contracts";
import { formatTaskNotification } from "../runtime-task/notification.js";

const REMAINING_ITEMS_BLOCK_PATTERN = new RegExp(
  `<${SUBAGENT_REMAINING_ITEMS_TAG}>([\\s\\S]*?)(?:</${SUBAGENT_REMAINING_ITEMS_TAG}>|$)`,
  "g",
);

/**
 * 子 agent 在最终答复末尾用 `<remaining_items>` 列出没做完的条目。块从正文剥离，
 * 条目交给父 agent 作为 `incomplete`，父 agent 才知道应 SendMessage 续接而不是重开。
 */
export function extractSubagentRemainingItems(response: string): {
  incomplete?: AgentIncompleteReport;
  text: string;
} {
  const remainingItems: string[] = [];
  let sawBlock = false;
  const text = response.replace(REMAINING_ITEMS_BLOCK_PATTERN, (_match, body: string) => {
    sawBlock = true;
    for (const line of body.split(/\r?\n/)) {
      const item = line.replace(/^\s*(?:[-*•]|\d+[.)])\s*/, "").trim();
      if (item) remainingItems.push(item);
    }
    return "";
  });
  if (!sawBlock) return { text: response };
  return {
    text: text.trimEnd(),
    ...(remainingItems.length > 0 ? { incomplete: { remainingItems } } : {}),
  };
}

export function buildSubagentContextReport(input: {
  contextWindow?: number;
  cumulativeBilledTokens?: number;
  events: readonly SessionEvent[];
}): AgentContextReport {
  let compactionCount = 0;
  let peakContextTokens: number | undefined;
  let stopReason: string | undefined;
  for (const event of input.events) {
    if (event.type === SessionEventType.CompactBoundary) {
      compactionCount += 1;
      continue;
    }
    if (event.type !== SessionEventType.ModelComplete || !isRecord(event.payload)) continue;
    const payload = event.payload;
    // compact summary 请求和工具内部嵌套用量不代表 agent 自己的工作上下文。
    if (payload.querySource === "compact" || payload.stopReason === "tool_internal") continue;
    if (isRecord(payload.usage)) {
      const contextTokens = getModelUsageContextTokens(payload.usage as ModelUsage);
      if (contextTokens !== undefined) {
        peakContextTokens = Math.max(peakContextTokens ?? 0, contextTokens);
      }
    }
    if (typeof payload.stopReason === "string") stopReason = payload.stopReason;
  }
  return {
    ...(input.cumulativeBilledTokens === undefined
      ? {}
      : { cumulativeBilledTokens: input.cumulativeBilledTokens }),
    ...(peakContextTokens === undefined ? {} : { peakContextTokens }),
    ...(input.contextWindow === undefined || !Number.isFinite(input.contextWindow)
      ? {}
      : { contextWindow: Math.floor(input.contextWindow) }),
    compactionCount,
    ...(stopReason ? { stopReason } : {}),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export type LocalAgentTaskNotificationStatus =
  | "completed"
  | "failed"
  | "stopped";

export interface LocalAgentTaskNotificationInput {
  agentId: string;
  agentType: string;
  contextReport?: AgentContextReport;
  description: string;
  error?: string;
  incomplete?: AgentIncompleteReport;
  outputFile: string;
  parentToolCallId: string;
  result?: string;
  status: LocalAgentTaskNotificationStatus;
  totalDurationMs?: number;
  totalTokens?: number;
  totalToolUseCount?: number;
  usage?: ModelUsage;
}

export function formatLocalAgentTaskNotification(
  input: LocalAgentTaskNotificationInput,
): string {
  return formatTaskNotification({
    agentId: input.agentId,
    description: input.description,
    error: input.error,
    ...(input.incomplete ? { incomplete: input.incomplete } : {}),
    outputFile: input.outputFile,
    result: input.result,
    status: input.status,
    subagentType: input.agentType,
    summary: formatLocalAgentNotificationSummary(input),
    taskId: input.agentId,
    taskType: "local_agent",
    toolUseId: input.parentToolCallId,
    usage: {
      ...(input.contextReport ? { contextReport: input.contextReport } : {}),
      durationMs: input.totalDurationMs,
      modelUsage: input.usage,
      toolUseCount: input.totalToolUseCount,
      totalTokens: input.totalTokens,
    },
  });
}

function formatLocalAgentNotificationSummary(
  input: Pick<
    LocalAgentTaskNotificationInput,
    "agentType" | "description" | "error" | "status"
  >,
): string {
  const summary = `Agent ${input.agentType} task "${input.description}" ${input.status}.`;
  const error =
    input.status === "failed" && input.error?.trim() ? input.error : undefined;
  // 修复原因：失败 summary 是 Agent 卡“子智能体输出”的来源，必须保留原前缀并
  // 直接追加与 <error> 相同的失败原因；completed/stopped 文案保持不变。
  return error ? `${summary} ${error}` : summary;
}
