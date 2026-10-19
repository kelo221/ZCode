import type { AgentRuntimeInternal } from "../internal.js";
import { rebuildContextPrefix } from "./context-refresh.js";

/**
 * 主 Agent system prompt 覆盖（GPUI 专属入口，Electron 从不调用）。
 *
 * 覆盖走 builder 既有的 `customSystemPrompt` 唯一路径：整段替换默认正文与全部 dynamic
 * system 段，CLI prefix、skills、AGENTS.md 与日期照常保留。子代理（subagentContext）与工作流
 * 子代理（workflowActor，与 customSystemPrompt 互斥）不接受覆盖。
 *
 * 活动轮次中途不重建前缀（与 language/outputStyle 一致）；改为记待办，轮次结束时补建，
 * 保证下一轮必然生效。
 */
const rebuildAfterTurn = new WeakSet<AgentRuntimeInternal>();

export function setSystemPromptOverride(
  this: AgentRuntimeInternal,
  prompt: string | undefined,
): boolean {
  if (this.config.subagentContext || this.config.workflowActor) return false;
  const next = prompt?.trim() ? prompt : undefined;
  if (this.config.systemPrompt === next) return true;
  this.config.systemPrompt = next;
  if (this.activeTurn) {
    rebuildAfterTurn.add(this);
  } else {
    rebuildContextPrefix(this);
  }
  return true;
}

/** 轮次结束钩子：补建中途变更的 system prompt 前缀。 */
export function flushPendingSystemPromptRebuild(runtime: AgentRuntimeInternal): void {
  if (!rebuildAfterTurn.delete(runtime)) return;
  rebuildContextPrefix(runtime);
}

/**
 * 不带覆盖时本会话的默认 system 正文（不含 CLI prefix 行），供客户端编辑器预填。
 * 只构建预览，不写回 envInfo、不替换消息历史。
 */
export function previewDefaultSystemPrompt(this: AgentRuntimeInternal): string {
  const saved = this.config.systemPrompt;
  this.config.systemPrompt = undefined;
  try {
    const snapshot =
      this.contextSourceSnapshot ?? this.createConfigOnlyContextSnapshot(this.workingDirectory);
    const builder = this.createContextBuilderFromSnapshot(snapshot, this.memoryRoot, {
      memoryIndexContent: this.memoryIndexContent,
      persistEnvInfo: false,
    });
    return defaultSystemBody(builder.build().sections);
  } finally {
    this.config.systemPrompt = saved;
  }
}

function defaultSystemBody(
  sections: readonly { injectionTarget: string; source: string; content: string }[],
): string {
  return sections
    .filter((section) => section.injectionTarget === "system" && section.source !== "cli_prefix")
    .map((section) => section.content)
    .join("\n\n");
}
