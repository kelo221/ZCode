// ============================================================
// Diagnostics Tool Handler（Fork，GPUI 门控）
// ============================================================
// 按需完整诊断：Rust 启动或加入一次项目检查，C# 拉取诊断，TypeScript 只有逐文件结果。
// 端口在场即注册；语言服务器的启动、容量与空闲关闭由 bootstrap 的 DiagnosticsManager 负责。

import {
  DIAGNOSTICS_DEFAULT_WAIT_SECONDS,
  DIAGNOSTICS_MAX_WAIT_SECONDS,
  DIAGNOSTICS_TOOL_NAME,
  DiagnosticsInputJsonSchema,
  DiagnosticsInputSchema,
  DiagnosticsOutputJsonSchema,
  DiagnosticsOutputSchema,
  type DiagnosticsOutput,
  type ModelMessageContent,
} from "@zcode/contracts";
import { formatDiagnosticsBlock } from "../diagnostics-format.js";
import { resolveWorkspacePath } from "../path-policy.js";
import type { ToolEntry, ToolHandler, ToolHandlerFailure } from "../types.js";

const DIAGNOSTICS_UNAVAILABLE_ERROR_CODE = 44;
const DIAGNOSTICS_MODEL_BYTES = 24_000;
const DIAGNOSTICS_PER_FILE_LINES = 60;
const DIAGNOSTICS_PER_FILE_BYTES = 8 * 1024;
const DIAGNOSTICS_TIMEOUT_MS = (DIAGNOSTICS_MAX_WAIT_SECONDS + 15) * 1000;

const DIAGNOSTICS_DESCRIPTION = `Run a full language-server check on up to 20 files and return their errors and warnings.

- Rust (rust-analyzer): starts or joins one \`cargo check\` for the project. \`check="complete"\` means that check finished after the current contents were synced; \`incomplete\` means it did not (deadline, or files changed during the check).
- C# (csharp-ls) and TypeScript/JavaScript (typescript-language-server): per-file analysis; TypeScript has no project check.
- \`freshness="confirmed"\` means the server answered for the current file contents; \`best_effort\` means it may lag behind.
- A file with no answer is never clean: look at \`timed_out\` and \`status\`.
- \`wait_seconds\` (default ${DIAGNOSTICS_DEFAULT_WAIT_SECONDS}, max ${DIAGNOSTICS_MAX_WAIT_SECONDS}) is one deadline shared by all files.`;

const diagnosticsHandler: ToolHandler = async (input, context) => {
  const parsed = DiagnosticsInputSchema.parse(input);
  const port = context.diagnosticsPort;
  if (port === undefined) {
    return {
      result: false,
      errorCode: DIAGNOSTICS_UNAVAILABLE_ERROR_CODE,
      message: `${DIAGNOSTICS_TOOL_NAME} is unavailable in this session. Run the project's build or type checker with Bash instead.`,
    } satisfies ToolHandlerFailure;
  }
  const files = parsed.paths.map((inputPath) =>
    resolveWorkspacePath({
      inputPath,
      operation: "read",
      workingDirectory: context.workingDirectory,
      workspaceRoot: context.workspaceRoot,
    }),
  );
  const waitSeconds = parsed.wait_seconds ?? DIAGNOSTICS_DEFAULT_WAIT_SECONDS;
  const results = await port.collect(
    { files, mode: "full", deadline: Date.now() + waitSeconds * 1000 },
    { signal: context.abortSignal },
  );
  return { files: results } satisfies DiagnosticsOutput;
};

export function formatDiagnosticsToolOutput(
  output: DiagnosticsOutput,
  workingDirectory: string | undefined,
): string {
  if (output.files.length === 0) return "No files were checked.";
  return output.files
    .map((result) =>
      formatDiagnosticsBlock(result, workingDirectory, {
        maxLines: DIAGNOSTICS_PER_FILE_LINES,
        maxBytes: DIAGNOSTICS_PER_FILE_BYTES,
      }),
    )
    .join("\n");
}

function formatDiagnosticsModelContent(output: unknown): ModelMessageContent {
  const parsed = DiagnosticsOutputSchema.safeParse(output);
  return parsed.success
    ? formatDiagnosticsToolOutput(parsed.data, undefined)
    : "Diagnostics returned an invalid result.";
}

export const diagnosticsToolEntry: ToolEntry = {
  capability: "Collect language-server errors and warnings for files",
  metadata: {
    name: DIAGNOSTICS_TOOL_NAME,
    description: DIAGNOSTICS_DESCRIPTION,
    readOnly: true,
    destructive: false,
    concurrentSafe: true,
    timeoutMs: DIAGNOSTICS_TIMEOUT_MS,
    maxOutputBytes: DIAGNOSTICS_MODEL_BYTES,
    sideEffectScope: "none",
    riskLevel: "low",
    needsApproval: false,
    shouldDefer: true,
  },
  handler: diagnosticsHandler,
  inputSchema: DiagnosticsInputJsonSchema,
  outputSchema: DiagnosticsOutputJsonSchema,
  runtimeInputSchema: DiagnosticsInputSchema,
  runtimeOutputSchema: DiagnosticsOutputSchema,
  formatModelContent: formatDiagnosticsModelContent,
  permission: {
    permission: "read",
    reason: `${DIAGNOSTICS_TOOL_NAME} only asks language servers about files and has no side effects`,
    riskLevel: "low",
    sideEffectScope: "none",
    needsApproval: false,
    patternSources: ["toolName"],
    alwaysAllowPatternSources: ["toolName"],
    denyPriority: "beforeAsk",
  },
  resultBudget: {
    maxInlineBytes: DIAGNOSTICS_MODEL_BYTES,
    maxModelBytes: DIAGNOSTICS_MODEL_BYTES,
    strategy: "truncate",
    preview: { maxBytes: DIAGNOSTICS_MODEL_BYTES, direction: "head" },
  },
  timeout: {
    kind: "timed",
    defaultMs: DIAGNOSTICS_TIMEOUT_MS,
    maxMs: DIAGNOSTICS_TIMEOUT_MS,
    allowCallOverride: false,
  },
  cancellation: {
    supported: true,
    cleanup: "none",
    userVisibleMessage: `${DIAGNOSTICS_TOOL_NAME} was cancelled before results were returned`,
  },
  trace: {
    required: true,
    propagateToAdapters: false,
    recordInput: "summary",
    recordOutput: "summary",
  },
};
