// ============================================================
// FindFiles / SearchCode Tools - fff 常驻索引搜索（GPUI 专属）
// ============================================================
//
// 只在注入了 CodeSearchPort 的会话里注册（见 code-search.port.ts）。

import { z } from "zod";
import { toToolJsonSchema } from "./json-schema.js";

export const FIND_FILES_TOOL_NAME = "FindFiles";
export const SEARCH_CODE_TOOL_NAME = "SearchCode";
export const CODE_SEARCH_DEFAULT_MAX_RESULTS = 20;
export const CODE_SEARCH_MAX_RESULTS = 100;
export const CODE_SEARCH_MAX_CONTEXT = 20;
export const CODE_SEARCH_MAX_PATTERNS = 20;

const maxResults = z.number().int().min(1).max(CODE_SEARCH_MAX_RESULTS).optional();
const cursor = z.string().trim().min(1).max(64).optional();

export const FindFilesInputSchema = z
  .object({
    query: z.string().trim().min(1).max(500),
    maxResults,
    cursor,
  })
  .strict();

export type FindFilesInput = z.infer<typeof FindFilesInputSchema>;

export const FindFilesInputJsonSchema = toToolJsonSchema(FindFilesInputSchema);

export const SearchCodeInputSchema = z
  .object({
    query: z.string().min(1).max(1_000).optional(),
    patterns: z.array(z.string().min(1).max(500)).min(1).max(CODE_SEARCH_MAX_PATTERNS).optional(),
    constraints: z.string().trim().min(1).max(500).optional(),
    output_mode: z.enum(["content", "files_with_matches", "count"]).optional(),
    context: z.number().int().min(0).max(CODE_SEARCH_MAX_CONTEXT).optional(),
    maxResults,
    cursor,
  })
  .strict()
  .refine((input) => (input.query === undefined) !== (input.patterns === undefined), {
    message: "Pass exactly one of `query` or `patterns`.",
  });

export type SearchCodeInput = z.infer<typeof SearchCodeInputSchema>;

export const SearchCodeInputJsonSchema = toToolJsonSchema(SearchCodeInputSchema);

/** 两个工具的模型可见输出都是格式化后的文本。 */
export const CodeSearchOutputSchema = z
  .object({
    text: z.string(),
    count: z.number().int().min(0),
  })
  .strict();

export type CodeSearchOutput = z.infer<typeof CodeSearchOutputSchema>;

export const CodeSearchOutputJsonSchema = toToolJsonSchema(CodeSearchOutputSchema);
