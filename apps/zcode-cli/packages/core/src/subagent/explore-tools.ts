// 对齐文件搜索型子 agent 的工具面。direct branch 会暴露 Glob/Grep；
// embedded search branch 则通过 Bash find/grep 接管搜索。注意：白名单刻意不含任何
// 文件写工具（Write/Edit/ApplyPatch），因此 Bash 是唯一的副作用入口，只读语义靠 Explore prompt 约束。
export const EXPLORE_AGENT_ALLOWED_TOOLS = [
  "ToolSearch",
  "DeferredToolCall",
  "Bash",
  "Glob",
  "Grep",
  "Read",
  "WebFetch",
  "WebSearch",
  "TodoWrite",
] as const;

// GPUI fff 索引搜索：只有注入了 CodeSearchPort 的会话才把它们加进 Explore 工具面，
// 其余宿主（Electron）的允许列表与事件载荷保持不变。
export const EXPLORE_AGENT_CODE_SEARCH_TOOLS = ["FindFiles", "SearchCode"] as const;

export type ExploreAgentAllowedTool =
  | (typeof EXPLORE_AGENT_ALLOWED_TOOLS)[number]
  | (typeof EXPLORE_AGENT_CODE_SEARCH_TOOLS)[number];

export const EXPLORE_AGENT_EMBEDDED_SEARCH_ALLOWED_TOOLS = [
  "ToolSearch",
  "DeferredToolCall",
  "Bash",
  "Read",
  "WebFetch",
  "WebSearch",
  "TodoWrite",
] as const;

const EXPLORE_AGENT_DESCRIPTION_TOOL_PRIORITY = [
  "Glob",
  "Grep",
  "Read",
  "Bash",
  "WebFetch",
  "WebSearch",
  "TodoWrite",
] as const satisfies readonly ExploreAgentAllowedTool[];
const EXPLORE_AGENT_DESCRIPTION_TOOL_PRIORITY_SET = new Set<ExploreAgentAllowedTool>(
  EXPLORE_AGENT_DESCRIPTION_TOOL_PRIORITY,
);

export function buildExploreAllowedTools(
  options: {
    embeddedSearchEnabled?: boolean;
    codeSearch?: boolean;
  } = {},
): readonly ExploreAgentAllowedTool[] {
  const tools: readonly ExploreAgentAllowedTool[] = options.embeddedSearchEnabled
    ? EXPLORE_AGENT_EMBEDDED_SEARCH_ALLOWED_TOOLS
    : EXPLORE_AGENT_ALLOWED_TOOLS;
  if (!options.codeSearch) return tools;
  const bash = tools.indexOf("Bash") + 1;
  return [...tools.slice(0, bash), ...EXPLORE_AGENT_CODE_SEARCH_TOOLS, ...tools.slice(bash)];
}

export function formatExploreAllowedToolsForAgentDescription(
  options: {
    embeddedSearchEnabled?: boolean;
  } = {},
): string {
  const allowedTools = buildExploreAllowedTools(options);
  const allowedToolSet = new Set(allowedTools);
  const prioritizedTools = EXPLORE_AGENT_DESCRIPTION_TOOL_PRIORITY.filter((tool) =>
    allowedToolSet.has(tool),
  );
  const unprioritizedTools = allowedTools.filter(
    (tool) =>
      tool !== "ToolSearch" &&
      tool !== "DeferredToolCall" &&
      !EXPLORE_AGENT_DESCRIPTION_TOOL_PRIORITY_SET.has(tool),
  );
  return [...prioritizedTools, ...unprioritizedTools].join(", ");
}
