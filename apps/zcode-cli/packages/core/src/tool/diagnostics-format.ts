// 语言服务器诊断的模型可见文本（编辑后附加块与 Diagnostics 工具共用）。
// 计数取自完整结果，列表再按行数与字节截断，因此截断不会改变计数。

import { isAbsolute, relative } from "node:path";
import type { FileDiagnostic, FileDiagnosticsResult } from "@zcode/contracts";

export const DIAGNOSTICS_TRAILER_MAX_LINES = 20;
export const DIAGNOSTICS_TRAILER_MAX_BYTES = 4 * 1024;
const MESSAGE_MAX_CHARS = 300;

export interface DiagnosticsFormatLimits {
  maxLines: number;
  maxBytes: number;
}

/** unsupported / unavailable 不值得占用模型上下文：编辑后附加块直接省略。 */
export function isReportableDiagnosticsResult(result: FileDiagnosticsResult): boolean {
  return result.availability !== "unsupported" && result.availability !== "unavailable";
}

export function formatDiagnosticsBlock(
  result: FileDiagnosticsResult,
  workingDirectory: string | undefined,
  limits: DiagnosticsFormatLimits = {
    maxLines: DIAGNOSTICS_TRAILER_MAX_LINES,
    maxBytes: DIAGNOSTICS_TRAILER_MAX_BYTES,
  },
): string {
  const attributes = [
    `file="${displayPath(result.filePath, workingDirectory)}"`,
    ...(result.server ? [`server="${result.server}"`] : []),
    `status="${result.availability}"`,
    `freshness="${result.freshness}"`,
    `check="${result.check}"`,
    ...(result.timedOut ? ['timed_out="true"'] : []),
  ];
  const lines = [`<diagnostics ${attributes.join(" ")}>`, summaryLine(result)];
  const body: string[] = [];
  for (const diagnostic of result.diagnostics) body.push(diagnosticLine(diagnostic, result));
  if (result.note) body.push(`note: ${result.note}`);

  let bytes = byteLength(lines.join("\n")) + byteLength("</diagnostics>") + 64;
  let shown = 0;
  for (const line of body) {
    const lineBytes = byteLength(line) + 1;
    if (shown >= limits.maxLines || bytes + lineBytes > limits.maxBytes) break;
    lines.push(line);
    bytes += lineBytes;
    shown += 1;
  }
  if (shown < body.length) lines.push(`(${body.length - shown} more lines omitted)`);
  lines.push("</diagnostics>");
  return lines.join("\n");
}

function summaryLine(result: FileDiagnosticsResult): string {
  if (result.availability === "starting") {
    return "The language server is still starting or loading the project; no result yet.";
  }
  if (result.availability === "capacity") {
    return "All language servers are busy; no result for this file yet.";
  }
  if (result.availability !== "ok") return "No diagnostics are available for this file.";
  const counts = `${plural(result.errorCount, "error")}, ${plural(result.warningCount, "warning")}`;
  const other =
    result.knownOtherFileErrors > 0
      ? ` The server also knows of ${plural(result.knownOtherFileErrors, "error")} in other files${
          result.check === "complete" ? "" : " (not a project total)"
        }.`
      : "";
  if (result.errorCount + result.warningCount === 0) {
    if (result.timedOut)
      return `No answer before the deadline; this is not a clean result.${other}`;
    return result.freshness === "confirmed"
      ? `No errors or warnings.${other}`
      : `No errors or warnings reported so far (unconfirmed).${other}`;
  }
  return `${counts}.${other}`;
}

function diagnosticLine(diagnostic: FileDiagnostic, result: FileDiagnosticsResult): string {
  const code = diagnostic.code ? `[${diagnostic.code}] ` : "";
  const message = firstLine(diagnostic.message);
  const notes: string[] = [];
  if (diagnostic.source) notes.push(diagnostic.source);
  if (diagnostic.stale) {
    const at = result.lastFullCheckAt ? ` at ${result.lastFullCheckAt}` : "";
    notes.push(`from the last full check${at}, may be outdated`);
  }
  const suffix = notes.length > 0 ? ` (${notes.join(", ")})` : "";
  return `${diagnostic.severity} ${diagnostic.line}:${diagnostic.column} ${code}${message}${suffix}`;
}

function firstLine(message: string): string {
  const line = message.split(/\r?\n/u, 1)[0]?.trim() ?? "";
  return line.length > MESSAGE_MAX_CHARS ? `${line.slice(0, MESSAGE_MAX_CHARS)}…` : line;
}

function displayPath(filePath: string, workingDirectory: string | undefined): string {
  if (!workingDirectory) return filePath;
  const rel = relative(workingDirectory, filePath);
  if (!rel || rel.startsWith("..") || isAbsolute(rel)) return filePath;
  return rel.replace(/\\/gu, "/");
}

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function byteLength(text: string): number {
  return Buffer.byteLength(text, "utf8");
}
