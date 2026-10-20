// ============================================================
// DiagnosticsPort - 语言服务器诊断（Fork，GPUI 门控）
// ============================================================
//
// 合同见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md。只有 Host 下发
// lspDiagnosticsEnabled 的会话才注入；未注入时 Edit/Write 不追加诊断尾部，也不注册
// Diagnostics 工具。

import { z } from "zod";

export const DIAGNOSTICS_AVAILABILITY = [
  "ok",
  "unsupported",
  "unavailable",
  "starting",
  "capacity",
] as const;
export const DIAGNOSTICS_FRESHNESS = ["confirmed", "best_effort"] as const;
export const DIAGNOSTICS_CHECK = [
  "not_requested",
  "complete",
  "incomplete",
  "not_supported",
] as const;

export type DiagnosticsAvailability = (typeof DIAGNOSTICS_AVAILABILITY)[number];
/** confirmed：对应我们同步的内容；best_effort：服务器未给版本，或晚于截止时间仍未回应。 */
export type DiagnosticsFreshness = (typeof DIAGNOSTICS_FRESHNESS)[number];
/** 项目级编译检查（rust-analyzer flycheck）是否覆盖本结果。 */
export type DiagnosticsCheck = (typeof DIAGNOSTICS_CHECK)[number];
export type DiagnosticsMode = "quick" | "full";

export const FileDiagnosticSchema = z
  .object({
    severity: z.enum(["error", "warning"]),
    /** 1-based。 */
    line: z.number().int().min(1),
    /** 1-based，UTF-16 列（与 LSP 默认 position encoding 一致）。 */
    column: z.number().int().min(1),
    code: z.string().max(200).optional(),
    source: z.string().max(100).optional(),
    message: z.string().max(4_000),
    /** 来自上一次完整检查（例如 rustc），不代表当前内容。 */
    stale: z.boolean().optional(),
  })
  .strict();

export type FileDiagnostic = z.infer<typeof FileDiagnosticSchema>;

export const FileDiagnosticsResultSchema = z
  .object({
    filePath: z.string().min(1),
    server: z.string().max(100).optional(),
    availability: z.enum(DIAGNOSTICS_AVAILABILITY),
    freshness: z.enum(DIAGNOSTICS_FRESHNESS),
    check: z.enum(DIAGNOSTICS_CHECK),
    timedOut: z.boolean(),
    /** 计数在截断前得出；diagnostics 可能已被截断。 */
    errorCount: z.number().int().min(0),
    warningCount: z.number().int().min(0),
    diagnostics: z.array(FileDiagnosticSchema),
    /** 服务器当前已知的其它文件错误数；check 未 complete 时不是项目总数。 */
    knownOtherFileErrors: z.number().int().min(0),
    /** ISO 时间：stale 诊断所属完整检查的结束时间。 */
    lastFullCheckAt: z.string().optional(),
    note: z.string().max(500).optional(),
  })
  .strict();

export type FileDiagnosticsResult = z.infer<typeof FileDiagnosticsResultSchema>;

export interface DiagnosticsRequest {
  /** 绝对路径。 */
  files: string[];
  mode: DiagnosticsMode;
  /** 整个请求共用一个截止时间（epoch ms），多文件不叠加等待。 */
  deadline: number;
}

export interface DiagnosticsPort {
  collect(
    request: DiagnosticsRequest,
    options?: { signal?: AbortSignal },
  ): Promise<FileDiagnosticsResult[]>;
}
