// ============================================================
// Diagnostics Tool - 按需完整诊断（Fork，GPUI 门控）
// ============================================================
//
// 只在注入了 DiagnosticsPort 的会话里注册（见 diagnostics.port.ts）。

import { z } from "zod";
import { FileDiagnosticsResultSchema } from "../interfaces/diagnostics.port.js";
import { toToolJsonSchema } from "./json-schema.js";

export const DIAGNOSTICS_TOOL_NAME = "Diagnostics";
export const DIAGNOSTICS_MAX_FILES = 20;
export const DIAGNOSTICS_DEFAULT_WAIT_SECONDS = 30;
export const DIAGNOSTICS_MAX_WAIT_SECONDS = 120;

export const DiagnosticsInputSchema = z
  .object({
    paths: z.array(z.string().trim().min(1).max(4_096)).min(1).max(DIAGNOSTICS_MAX_FILES),
    wait_seconds: z.number().int().min(1).max(DIAGNOSTICS_MAX_WAIT_SECONDS).optional(),
  })
  .strict();

export type DiagnosticsInput = z.infer<typeof DiagnosticsInputSchema>;

export const DiagnosticsInputJsonSchema = toToolJsonSchema(DiagnosticsInputSchema);

export const DiagnosticsOutputSchema = z
  .object({
    files: z.array(FileDiagnosticsResultSchema),
  })
  .strict();

export type DiagnosticsOutput = z.infer<typeof DiagnosticsOutputSchema>;

export const DiagnosticsOutputJsonSchema = toToolJsonSchema(DiagnosticsOutputSchema);
