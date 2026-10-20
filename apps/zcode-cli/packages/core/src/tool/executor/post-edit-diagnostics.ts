// 编辑后诊断（Fork，GPUI 门控，见 docs/design/v2/tool/lsp-diagnostics.md）。
//
// 只在注入了 DiagnosticsPort 的会话里、文件变更工具成功之后运行 quick 收集：一个 3 秒截止时间，
// 任何失败都只记日志，绝不让已经落盘的编辑失败。同一份结果同时生成模型可见附加块与
// file_diff display 的 diagnostics 摘要，计数在截断前得出。

import type {
  DiagnosticsPort,
  FileDiagnosticsResult,
  Logger,
  ToolResultDisplayPayload,
} from "@zcode/contracts";
import { formatDiagnosticsBlock, isReportableDiagnosticsResult } from "../diagnostics-format.js";
import type { ToolResultSerialization } from "../types.js";

export const POST_EDIT_DIAGNOSTICS_DEADLINE_MS = 3_000;
const POST_EDIT_MAX_FILES = 5;

export interface PostEditDiagnostics {
  results: FileDiagnosticsResult[];
  trailer: string;
}

/** Edit/Write 输出 `filePath + structuredPatch`；ApplyPatch 输出 `files[]`（删除的文件不诊断）。 */
export function mutatedFilePaths(output: unknown): string[] {
  if (!isRecord(output)) return [];
  if (
    typeof output.filePath === "string" &&
    output.filePath &&
    Array.isArray(output.structuredPatch)
  ) {
    return [output.filePath];
  }
  if (!Array.isArray(output.files) || !Array.isArray(output.structuredPatch)) return [];
  const paths: string[] = [];
  for (const change of output.files) {
    if (!isRecord(change) || change.type === "delete") continue;
    const path =
      typeof change.movePath === "string" && change.movePath ? change.movePath : change.filePath;
    if (typeof path === "string" && path) paths.push(path);
  }
  return paths.slice(0, POST_EDIT_MAX_FILES);
}

export async function collectPostEditDiagnostics(options: {
  port: DiagnosticsPort | undefined;
  output: unknown;
  workingDirectory: string | undefined;
  signal: AbortSignal | undefined;
  logger?: Logger;
  now?: () => number;
}): Promise<PostEditDiagnostics | undefined> {
  if (!options.port) return undefined;
  const files = mutatedFilePaths(options.output);
  if (files.length === 0) return undefined;
  try {
    const now = options.now ?? Date.now;
    const results = await options.port.collect(
      { files, mode: "quick", deadline: now() + POST_EDIT_DIAGNOSTICS_DEADLINE_MS },
      { signal: options.signal },
    );
    const reportable = results.filter(isReportableDiagnosticsResult);
    if (reportable.length === 0) return undefined;
    return {
      results: reportable,
      trailer: reportable
        .map((result) => formatDiagnosticsBlock(result, options.workingDirectory))
        .join("\n"),
    };
  } catch (error) {
    if (options.signal?.aborted) return undefined;
    options.logger?.warn("Post-edit diagnostics failed", {
      error: error instanceof Error ? error.message : String(error),
      event: "tool.post_edit_diagnostics.failed",
      module: "core.tool.executor",
    });
    return undefined;
  }
}

/** 附加块只追加在文本结果后；文件变更工具的模型结果是短确认文本。 */
export function appendDiagnosticsTrailer(
  serialization: ToolResultSerialization,
  trailer: string,
): ToolResultSerialization {
  const suffix = `\n\n${trailer}`;
  const modelContent = serialization.modelContent ?? serialization.content;
  const content = `${serialization.content}${suffix}`;
  const nextModelContent =
    typeof modelContent === "string"
      ? `${modelContent}${suffix}`
      : [...modelContent, { type: "text" as const, text: trailer }];
  return {
    ...serialization,
    content,
    modelContent: nextModelContent,
    returnedBytes: serialization.returnedBytes + Buffer.byteLength(suffix, "utf8"),
  };
}

export function withDiagnosticsDisplay(
  display: ToolResultDisplayPayload | undefined,
  diagnostics: PostEditDiagnostics | undefined,
): ToolResultDisplayPayload | undefined {
  if (!display || display.kind !== "file_diff" || !diagnostics) return display;
  const result = diagnostics.results.find((item) => samePath(item.filePath, display.filePath));
  if (!result) return display;
  return {
    ...display,
    diagnostics: {
      errors: result.errorCount,
      warnings: result.warningCount,
      availability: result.availability,
      freshness: result.freshness,
      check: result.check,
      ...(result.timedOut ? { timedOut: true } : {}),
    },
  };
}

function samePath(left: string, right: string): boolean {
  const normalize = (path: string): string => {
    const slashed = path.replace(/\\/gu, "/");
    return /^[a-z]:\//iu.test(slashed) ? slashed.toLowerCase() : slashed;
  };
  return normalize(left) === normalize(right);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
