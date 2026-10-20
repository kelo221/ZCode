import { createMessageId, type SubagentListEntry, type TraceContext } from "@zcode/contracts";
import {
  systemReminderAttachmentEntry,
  type RuntimeAttachmentEntry,
  type RuntimeMessageEntry,
} from "../../agent/message-history.js";
import { formatSubagentRosterReminder, mergeSessionSubagents } from "../../subagent/agent-index.js";
import { collectAgentListingAttachment } from "../../subagent/listing.js";
import type { AgentRuntimeInternal } from "../internal.js";
import { resolveRuntimeEmbeddedSearchEnabled } from "../methods/embedded-search-branch.js";
import { commitTurnRequestEntries } from "../methods/turn-output-token-continuation.js";
import type { TurnRequestState } from "../methods/turn-loop-state.js";

export function collectRuntimeAgentListing(
  runtime: AgentRuntimeInternal,
  entries: readonly RuntimeMessageEntry[],
  tools: readonly { name: string }[],
): RuntimeAttachmentEntry | undefined {
  if (!tools.some((tool) => tool.name === "Agent")) return undefined;
  return collectAgentListingAttachment({
    definitions: runtime.getAgentDefinitions(),
    entries,
    tools,
    embeddedSearchEnabled: resolveRuntimeEmbeddedSearchEnabled(runtime),
  });
}

/**
 * compact 会把 Agent 结果里的 agent id 一起摘要掉，而 agent listing 只公告类型、不含实例。
 * 把本 session 已有的 subagent 重新列进 reminder，模型才能继续用 SendMessage 续用它们。
 *
 * 磁盘读取在 compact 开始时启动，与摘要请求并行；检查点构建时只同步取结果，不再插入 I/O 等待
 * （检查点构建之后的重试计时与取消检查依赖它保持同步）。读取尚未完成时退回内存 registry。
 */
export function startRuntimeSubagentRosterRead(
  runtime: AgentRuntimeInternal,
): () => RuntimeAttachmentEntry | undefined {
  const port = runtime.subagentPort;
  if (!port?.listAgents) return () => undefined;
  let settled: SubagentListEntry[] | undefined;
  void port.listAgents(runtime.sessionId).then(
    (entries) => {
      settled = entries;
    },
    (error: unknown) => {
      runtime.logger?.warn("Subagent roster unavailable for post-compact reminder", {
        error: error instanceof Error ? error.message : String(error),
        event: "subagent.roster_unavailable",
        module: "core.runtime",
      });
    },
  );
  return () => {
    const content = formatSubagentRosterReminder(
      mergeSessionSubagents(settled ?? [], runtime.runtimeTaskRegistry, runtime.sessionId),
    );
    return content
      ? systemReminderAttachmentEntry("resume_referenced_session_context", content)
      : undefined;
  };
}

export async function appendRuntimeAgentListing(
  runtime: AgentRuntimeInternal,
  state: TurnRequestState,
  tools: readonly { name: string }[],
  traceContext: TraceContext,
): Promise<void> {
  const entry = collectRuntimeAgentListing(runtime, state.entries, tools);
  if (!entry) return;
  // 持久化成功后才提交到权威历史，失败不能让后续请求误判为已通知。
  await runtime.persistSyntheticUserNoticeForSession({
    messageID: createMessageId(),
    sessionId: runtime.sessionId,
    source: "agent_listing_delta",
    text: entry.content,
    metadata: { runtimeMessage: entry.metadata },
    traceContext,
  });
  commitTurnRequestEntries(runtime, state, [entry]);
}
