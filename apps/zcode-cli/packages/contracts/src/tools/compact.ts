// ============================================================
// Compact Tool - agent 在当前 turn 内请求压缩自己的上下文
// ============================================================
//
// 工具只记录请求；turn loop 在下一次模型请求前执行压缩
// （docs/design/v2/compact/README.md「Agent 请求的 compact」）。

import { z } from "zod";
import { toToolJsonSchema } from "./json-schema.js";

export const COMPACT_TOOL_NAME = "Compact";
export const COMPACT_INSTRUCTIONS_MAX_CHARS = 2_000;

export const CompactInputSchema = z
  .object({
    instructions: z.string().trim().min(1).max(COMPACT_INSTRUCTIONS_MAX_CHARS).optional(),
  })
  .strict();

export type CompactInput = z.infer<typeof CompactInputSchema>;

export const CompactInputJsonSchema = toToolJsonSchema(CompactInputSchema);

export const CompactOutputSchema = z
  .object({
    scheduled: z.literal(true),
    instructions: z.string().optional(),
  })
  .strict();

export type CompactOutput = z.infer<typeof CompactOutputSchema>;

export const CompactOutputJsonSchema = toToolJsonSchema(CompactOutputSchema);
