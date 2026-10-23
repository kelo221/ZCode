// ============================================================
// WorktreeIsolationPort - git worktree / jj workspace 隔离的子代理（Fork）
// ============================================================
// 合同见 docs/design/v2/tool/07-subagent.md「Worktree isolation 当前范围」与「jj 后端」。core 只经这个
// 端口调用 VCS；实现位于 adapters/src/git/worktree.ts、adapters/src/jj/workspace.ts，由
// adapters/src/vcs/auto-worktree.ts 按仓库选择，bootstrap 注入。

import { z } from "zod";

export const WorktreeVcsSchema = z.enum(["git", "jj"]);
export type WorktreeVcs = z.infer<typeof WorktreeVcsSchema>;

export const WorktreeRecordSchema = z
  .object({
    /** 缺省为 git（早于 jj 后端的记录没有这个字段）。 */
    vcs: WorktreeVcsSchema.optional(),
    /** git：`git rev-parse --show-toplevel`（从 linked worktree 启动时就是那个 worktree 的根）；jj：`jj root`。 */
    repoRoot: z.string().min(1),
    path: z.string().min(1),
    /** git branch，或 jj bookmark。 */
    branch: z.string().min(1),
    /** 创建时的起点提交 id（git HEAD / jj 父 workspace 的 `@-`）。branch 的比较基准。 */
    sourceHead: z.string().min(1),
    /** 最近一次 finalize 后 branch 指向的提交 id。 */
    resultHead: z.string().min(1).optional(),
    /** 最近一次 finalize 失败的原因；下一次 finalize 重试。 */
    finalizeError: z.string().max(2_000).optional(),
    /** 共享给 worktree 的 Cargo target 目录；用户显式设置 CARGO_TARGET_DIR 时不填。 */
    cargoTargetDir: z.string().min(1).optional(),
    /** child 命令的额外环境（例如 jj workspace 的 GIT_CEILING_DIRECTORIES）。 */
    env: z.record(z.string(), z.string()).optional(),
    /** finalize 发现没有任何改动时删除了 worktree 与 branch。 */
    removed: z.boolean().optional(),
  })
  .strict();

export type WorktreeRecord = z.infer<typeof WorktreeRecordSchema>;

export function worktreeVcs(record: Pick<WorktreeRecord, "vcs">): WorktreeVcs {
  return record.vcs ?? "git";
}

export type WorktreeIsolationErrorCode =
  | "not_a_repo"
  | "no_commits"
  | "create_failed"
  | "unavailable";

export class WorktreeIsolationError extends Error {
  constructor(
    readonly code: WorktreeIsolationErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "WorktreeIsolationError";
  }
}

export interface WorktreeFinalizeResult {
  record: WorktreeRecord;
  /** removed：没有任何改动，worktree 与 branch 已删除。committed：本次提交了未提交改动。kept：agent 自己已提交。 */
  outcome: "removed" | "committed" | "kept";
  /** `sourceHead..resultHead` 的 diff 统计行。 */
  diffStat?: string;
  commitsAhead: number;
  /** 提交或清理失败；文件与 branch 保留。 */
  error?: string;
}

export interface WorktreeIsolationPort {
  /** 在 cwd 所在仓库的起点提交上创建 worktree / workspace。失败抛 WorktreeIsolationError。 */
  create(input: { cwd: string; agentId: string }): Promise<WorktreeRecord>;
  /** worktree 缺失时从 branch（或 sourceHead）重建。失败抛 WorktreeIsolationError("unavailable")。 */
  ensure(record: WorktreeRecord): Promise<WorktreeRecord>;
  /** 提交未提交改动、无改动时清理。只在 child 真正结束后调用。 */
  finalize(record: WorktreeRecord, input: { message: string }): Promise<WorktreeFinalizeResult>;
}
