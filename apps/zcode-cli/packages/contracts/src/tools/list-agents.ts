// ============================================================
// ListAgents Tool - 列出当前 parent session 的 subagent 及其 agent id
// ============================================================

import { z } from "zod";
import { toToolJsonSchema } from "./json-schema.js";

export const LIST_AGENTS_TOOL_NAME = "ListAgents";

export const LIST_AGENTS_MIN_LIMIT = 1;
export const LIST_AGENTS_MAX_LIMIT = 50;
export const LIST_AGENTS_DEFAULT_LIMIT = 20;

/** 与 `SubagentTaskStatus` 同集；schema 是运行时校验面，所以在这里重新声明成 zod 枚举。 */
export const LIST_AGENTS_STATUSES = [
  "running",
  "completed",
  "failed",
  "cancelled",
  "killed",
  "stopped",
  "lost",
] as const;

export const ListAgentsInputSchema = z
  .object({
    limit: z
      .preprocess((value) => {
        if (typeof value !== "number" || !Number.isFinite(value)) return value;
        return Math.min(LIST_AGENTS_MAX_LIMIT, Math.max(LIST_AGENTS_MIN_LIMIT, Math.trunc(value)));
      }, z.number().int().min(LIST_AGENTS_MIN_LIMIT).max(LIST_AGENTS_MAX_LIMIT).default(LIST_AGENTS_DEFAULT_LIMIT))
      .describe(
        `Maximum number of subagents to return (${LIST_AGENTS_MIN_LIMIT}-${LIST_AGENTS_MAX_LIMIT}, default ${LIST_AGENTS_DEFAULT_LIMIT}). Most recently updated first.`,
      ),
    statuses: z
      .array(z.enum(LIST_AGENTS_STATUSES))
      .optional()
      .describe(
        'Optional status filter, e.g. ["running"] or ["failed", "stopped"]. Omit to list all.',
      ),
  })
  // 刻意没有 session 输入：只列当前 session 的 subagent。
  .strict();

export type ListAgentsInput = z.infer<typeof ListAgentsInputSchema>;

export const ListAgentsInputJsonSchema = toToolJsonSchema(ListAgentsInputSchema);

export const ListAgentsOutputSchema = z
  .object({
    agents: z.array(
      z
        .object({
          agentId: z.string(),
          agentType: z.string(),
          description: z.string(),
          status: z.enum(LIST_AGENTS_STATUSES),
          startedAt: z.string(),
          updatedAt: z.string(),
          live: z.boolean(),
          /** 隔离 worktree 仍在时的分支与路径；已移除（无改动）则省略。 */
          worktree: z.object({ branch: z.string(), path: z.string() }).strict().optional(),
        })
        .strict(),
    ),
    total: z.number().int().min(0),
  })
  .strict();

export type ListAgentsOutput = z.infer<typeof ListAgentsOutputSchema>;

export const ListAgentsOutputJsonSchema = toToolJsonSchema(ListAgentsOutputSchema);
