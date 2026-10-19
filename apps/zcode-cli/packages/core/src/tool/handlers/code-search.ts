// ============================================================
// FindFiles / SearchCode Tool Handlers
// ============================================================
// fff 常驻索引搜索（GPUI 专属，见 apps/zcode-gpui/PARITY.md「Built-in fff code search」）。
// 端口在场即注册；索引的打开与空闲关闭由端口实现（bootstrap manager）负责。

import {
  CODE_SEARCH_DEFAULT_MAX_RESULTS,
  CodeSearchOutputJsonSchema,
  CodeSearchOutputSchema,
  FIND_FILES_TOOL_NAME,
  FindFilesInputJsonSchema,
  FindFilesInputSchema,
  SEARCH_CODE_TOOL_NAME,
  SearchCodeInputJsonSchema,
  SearchCodeInputSchema,
  type CodeSearchFindResult,
  type CodeSearchOutcome,
  type CodeSearchOutput,
  type CodeSearchTextResult,
  type ModelMessageContent,
} from "@zcode/contracts";
import type { ToolEntry, ToolHandler, ToolHandlerFailure } from "../types.js";

const CODE_SEARCH_TIMEOUT_MS = 30_000;
const CODE_SEARCH_MODEL_BYTES = 20_000;
const CODE_SEARCH_ERROR_CODE = { UNAVAILABLE: 42, CURSOR_EXPIRED: 43 } as const;

const CONSTRAINT_HELP =
  "Narrow with constraint tokens in the query: `src/` (path segment), `*.ts` or `*.{ts,tsx}` (extension), `!test` or `!*.spec.ts` (exclude), `git:modified` (FindFiles only).";

const FIND_FILES_DESCRIPTION = `Fuzzy file search over an always-up-to-date index of this workspace. Prefer it over \`find\`/\`ls -R\`/Glob for locating files by name.

- Typo-tolerant; multiple words must all match. Results are ranked by match quality and git status, paths are relative to the workspace root.
- ${CONSTRAINT_HELP}
- Pass the returned \`cursor\` to get the next page.`;

const SEARCH_CODE_DESCRIPTION = `Content search over an always-up-to-date index of this workspace. Prefer it over \`grep\`/\`rg\` via Bash for searching code.

- \`query\`: plain text, or a regex when it contains regex syntax (Rust regex: no lookaround or backreferences). Smart case: case-insensitive unless the query has an uppercase letter. Falls back to fuzzy matching when nothing matches exactly.
- \`patterns\`: several literal strings matched with OR logic, instead of \`query\`.
- ${CONSTRAINT_HELP} Or pass them in \`constraints\`.
- \`output_mode\`: "content" (default, matching lines), "files_with_matches" or "count". \`context\` adds lines around each match.
- Paths are relative to the workspace root. Pass the returned \`cursor\` to get the next page.`;

function unavailableFailure(
  toolName: string,
  outcome: { reason: string; message: string },
): ToolHandlerFailure {
  if (outcome.reason === "cursor_expired") {
    return {
      result: false,
      errorCode: CODE_SEARCH_ERROR_CODE.CURSOR_EXPIRED,
      message: outcome.message,
    };
  }
  return {
    result: false,
    errorCode: CODE_SEARCH_ERROR_CODE.UNAVAILABLE,
    message: `${toolName} is unavailable: ${outcome.message}. Use Bash (rg/find) instead.`,
  };
}

const findFilesHandler: ToolHandler = async (input, context) => {
  const parsed = FindFilesInputSchema.parse(input);
  const port = context.codeSearchPort;
  if (port === undefined) {
    return unavailableFailure(FIND_FILES_TOOL_NAME, {
      reason: "unavailable",
      message: "this session has no code search index",
    });
  }
  const outcome = await port.findFiles(
    {
      query: parsed.query,
      maxResults: parsed.maxResults ?? CODE_SEARCH_DEFAULT_MAX_RESULTS,
      ...(parsed.cursor ? { cursor: parsed.cursor } : {}),
    },
    { signal: context.abortSignal },
  );
  if (!outcome.ok) return unavailableFailure(FIND_FILES_TOOL_NAME, outcome);
  return formatFindFiles(outcome) satisfies CodeSearchOutput;
};

const searchCodeHandler: ToolHandler = async (input, context) => {
  const parsed = SearchCodeInputSchema.parse(input);
  const port = context.codeSearchPort;
  if (port === undefined) {
    return unavailableFailure(SEARCH_CODE_TOOL_NAME, {
      reason: "unavailable",
      message: "this session has no code search index",
    });
  }
  const outputMode = parsed.output_mode ?? "content";
  const outcome = await port.searchCode(
    {
      ...(parsed.query !== undefined ? { query: parsed.query } : {}),
      ...(parsed.patterns !== undefined ? { patterns: parsed.patterns } : {}),
      ...(parsed.constraints ? { constraints: parsed.constraints } : {}),
      outputMode,
      context: parsed.context ?? 0,
      maxResults: parsed.maxResults ?? CODE_SEARCH_DEFAULT_MAX_RESULTS,
      ...(parsed.cursor ? { cursor: parsed.cursor } : {}),
    },
    { signal: context.abortSignal },
  );
  if (!outcome.ok) return unavailableFailure(SEARCH_CODE_TOOL_NAME, outcome);
  return formatSearchCode(outcome, outputMode) satisfies CodeSearchOutput;
};

export function formatFindFiles(
  result: CodeSearchOutcome<CodeSearchFindResult> & { ok: true },
): CodeSearchOutput {
  if (result.files.length === 0) return { text: "No matching files.", count: 0 };
  const lines = [`${result.files.length}/${result.totalMatched} matching files`];
  for (const file of result.files) {
    lines.push(file.gitStatus ? `${file.path} [${file.gitStatus}]` : file.path);
  }
  if (result.nextCursor) lines.push(`cursor: ${result.nextCursor}`);
  return { text: lines.join("\n"), count: result.files.length };
}

export function formatSearchCode(
  result: CodeSearchOutcome<CodeSearchTextResult> & { ok: true },
  outputMode: "content" | "files_with_matches" | "count",
): CodeSearchOutput {
  const lines: string[] = [];
  if (result.regexError) lines.push(`Regex did not compile (${result.regexError}).`);
  if (result.matches.length === 0) {
    lines.push("No matches.");
    return { text: lines.join("\n"), count: 0 };
  }
  if (result.fuzzyFallback) lines.push("No exact matches; showing fuzzy matches.");

  if (outputMode === "files_with_matches") {
    const files = [...new Set(result.matches.map((match) => match.path))];
    lines.push(`${files.length} files`, ...files);
    if (result.nextCursor) lines.push(`cursor: ${result.nextCursor}`);
    return { text: lines.join("\n"), count: files.length };
  }

  if (outputMode === "count") {
    const counts = new Map<string, number>();
    for (const match of result.matches) {
      counts.set(match.path, (counts.get(match.path) ?? 0) + 1);
    }
    lines.push(`${result.matches.length} matches in ${counts.size} files`);
    for (const [path, count] of counts) lines.push(`${path}: ${count}`);
    if (result.nextCursor) lines.push(`cursor: ${result.nextCursor}`);
    return { text: lines.join("\n"), count: result.matches.length };
  }

  lines.push(`${result.matches.length} matches${result.mode === "regex" ? " (regex)" : ""}`);
  let currentPath: string | undefined;
  for (const match of result.matches) {
    if (match.path !== currentPath) {
      lines.push(match.path);
      currentPath = match.path;
    }
    const before = match.before ?? [];
    before.forEach((text, index) => {
      lines.push(` ${match.line - before.length + index}-${text}`);
    });
    lines.push(` ${match.line}: ${match.text}`);
    (match.after ?? []).forEach((text, index) => {
      lines.push(` ${match.line + index + 1}-${text}`);
    });
  }
  if (result.nextCursor) lines.push(`cursor: ${result.nextCursor}`);
  return { text: lines.join("\n"), count: result.matches.length };
}

function formatCodeSearchModelContent(output: unknown): ModelMessageContent {
  const parsed = CodeSearchOutputSchema.safeParse(output);
  return parsed.success ? parsed.data.text : "Code search returned an invalid result.";
}

function codeSearchEntry(
  name: string,
  description: string,
  capability: string,
  handler: ToolHandler,
  inputSchema: ToolEntry["inputSchema"],
  runtimeInputSchema: ToolEntry["runtimeInputSchema"],
): ToolEntry {
  return {
    capability,
    metadata: {
      name,
      description,
      readOnly: true,
      destructive: false,
      concurrentSafe: true,
      timeoutMs: CODE_SEARCH_TIMEOUT_MS,
      maxOutputBytes: CODE_SEARCH_MODEL_BYTES,
      sideEffectScope: "none",
      riskLevel: "low",
      needsApproval: false,
    },
    handler,
    inputSchema,
    outputSchema: CodeSearchOutputJsonSchema,
    runtimeInputSchema,
    runtimeOutputSchema: CodeSearchOutputSchema,
    formatModelContent: formatCodeSearchModelContent,
    permission: {
      permission: "read",
      reason: `${name} only reads the workspace search index and has no side effects`,
      riskLevel: "low",
      sideEffectScope: "none",
      needsApproval: false,
      patternSources: ["toolName"],
      alwaysAllowPatternSources: ["toolName"],
      denyPriority: "beforeAsk",
    },
    resultBudget: {
      maxInlineBytes: CODE_SEARCH_MODEL_BYTES,
      maxModelBytes: CODE_SEARCH_MODEL_BYTES,
      strategy: "truncate",
      preview: { maxBytes: CODE_SEARCH_MODEL_BYTES, direction: "head" },
    },
    timeout: {
      kind: "timed",
      defaultMs: CODE_SEARCH_TIMEOUT_MS,
      maxMs: CODE_SEARCH_TIMEOUT_MS,
      allowCallOverride: false,
    },
    cancellation: {
      supported: true,
      cleanup: "none",
      userVisibleMessage: `${name} was cancelled before results were returned`,
    },
    trace: {
      required: true,
      propagateToAdapters: false,
      recordInput: "summary",
      recordOutput: "summary",
    },
  };
}

export const findFilesToolEntry = codeSearchEntry(
  FIND_FILES_TOOL_NAME,
  FIND_FILES_DESCRIPTION,
  "Find workspace files by fuzzy name match",
  findFilesHandler,
  FindFilesInputJsonSchema,
  FindFilesInputSchema,
);

export const searchCodeToolEntry = codeSearchEntry(
  SEARCH_CODE_TOOL_NAME,
  SEARCH_CODE_DESCRIPTION,
  "Search workspace file contents from the code search index",
  searchCodeHandler,
  SearchCodeInputJsonSchema,
  SearchCodeInputSchema,
);
