// ============================================================
// Compact Tool Handler
// ============================================================
// 只校验并回显请求；压缩由 turn loop 在下一次模型请求前执行
// （docs/design/v2/compact/README.md「Agent 请求的 compact」）。

import {
  COMPACT_TOOL_NAME,
  CompactInputJsonSchema,
  CompactInputSchema,
  CompactOutputJsonSchema,
  CompactOutputSchema,
  type CompactOutput,
  type ModelMessageContent,
} from "@zcode/contracts";
import type { ToolEntry, ToolExecutionResult, ToolHandler } from "../types.js";

const COMPACT_TIMEOUT_MS = 5_000;
const COMPACT_MODEL_BYTES = 1_000;

const COMPACT_DESCRIPTION =
  "Compact your own conversation context: earlier messages are replaced by a summary before your next step, and you keep working on the same task.";

const compactHandler: ToolHandler = async (input) => {
  const parsed = CompactInputSchema.parse(input);
  return (
    parsed.instructions === undefined
      ? { scheduled: true }
      : { scheduled: true, instructions: parsed.instructions }
  ) satisfies CompactOutput;
};

function formatCompactModelContent(): ModelMessageContent {
  return "Compaction scheduled. It runs before your next step; continue the task afterwards.";
}

/** 一个工具批次里成功的 Compact 调用；多次调用合并，后出现的 instructions 覆盖前面的。 */
export function requestedCompactFromResults(
  results: readonly ToolExecutionResult[],
): { instructions?: string } | undefined {
  let request: { instructions?: string } | undefined;
  for (const result of results) {
    if (result.toolName !== COMPACT_TOOL_NAME || !result.success) continue;
    const parsed = CompactOutputSchema.safeParse(result.output);
    if (!parsed.success) continue;
    request =
      parsed.data.instructions === undefined
        ? (request ?? {})
        : { instructions: parsed.data.instructions };
  }
  return request;
}

export const compactToolEntry: ToolEntry = {
  capability: "Compact this agent's own conversation context",
  metadata: {
    shouldDefer: true,
    name: COMPACT_TOOL_NAME,
    searchHint: "compact or summarize your own context when asked to",
    description: COMPACT_DESCRIPTION,
    modelInstructions: [
      "Call Compact only when the user, or the agent that sent you a message, explicitly asks you to compact. Automatic compaction already runs when your context fills up.",
      "Use `instructions` to say what the summary must keep, if the request named something.",
      "Compaction is not a reason to stop: after it runs, continue the task from the summary.",
    ],
    // 只改写本会话的上下文，不碰工作区，所以 plan 模式下也允许。
    allowedInPlanMode: true,
    readOnly: false,
    destructive: false,
    concurrentSafe: true,
    timeoutMs: COMPACT_TIMEOUT_MS,
    maxOutputBytes: COMPACT_MODEL_BYTES,
    sideEffectScope: "session",
    riskLevel: "low",
    needsApproval: false,
  },
  handler: compactHandler,
  inputSchema: CompactInputJsonSchema,
  outputSchema: CompactOutputJsonSchema,
  runtimeInputSchema: CompactInputSchema,
  runtimeOutputSchema: CompactOutputSchema,
  formatModelContent: formatCompactModelContent,
  permission: {
    permission: "session.compact",
    reason: "Compact replaces this session's earlier context with a summary",
    riskLevel: "low",
    sideEffectScope: "session",
    needsApproval: false,
    patternSources: ["toolName"],
    alwaysAllowPatternSources: ["toolName"],
    denyPriority: "beforeAsk",
  },
  resultBudget: {
    maxInlineBytes: COMPACT_MODEL_BYTES,
    maxModelBytes: COMPACT_MODEL_BYTES,
    strategy: "truncate",
    preview: { maxBytes: COMPACT_MODEL_BYTES, direction: "head" },
  },
  timeout: {
    kind: "timed",
    defaultMs: COMPACT_TIMEOUT_MS,
    maxMs: COMPACT_TIMEOUT_MS,
    allowCallOverride: false,
  },
  cancellation: {
    supported: true,
    cleanup: "none",
    userVisibleMessage: "Compact was cancelled",
  },
  trace: {
    required: true,
    propagateToAdapters: false,
    recordInput: "summary",
    recordOutput: "summary",
  },
};
