// ============================================================
// WorktreeIsolationPort - git worktree 隔离的子代理（Fork）
// ============================================================
// 合同见 docs/design/v2/tool/07-subagent.md「Worktree isolation 当前范围」。core 只经这个端口调用
// git；实现位于 adapters/src/git/worktree.ts，由 bootstrap 注入。

import { z } from "zod";

export const WorktreeRecordSchema = z
  .object({
    /** `git rev-parse --show-toplevel`：从 linked worktree 启动时就是那个 worktree 的根。 */
    repoRoot: z.string().min(1),
    path: z.string().min(1),
    branch: z.string().min(1),
    /** 创建时的 HEAD。branch 的比较基准。 */
    sourceHead: z.string().min(1),
    /** 最近一次 finalize 后 branch 的 HEAD。 */
    resultHead: z.string().min(1).optional(),
    /** 最近一次 finalize 失败的原因；下一次 finalize 重试。 */
    finalizeError: z.string().max(2_000).optional(),
    /** 共享给 worktree 的 Cargo target 目录；用户显式设置 CARGO_TARGET_DIR 时不填。 */
    cargoTargetDir: z.string().min(1).optional(),
    /** finalize 发现没有任何改动时删除了 worktree 与 branch。 */
    removed: z.boolean().optional(),
  })
  .strict();

export type WorktreeRecord = z.infer<typeof WorktreeRecordSchema>;

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
  /** `git diff --shortstat sourceHead resultHead`。 */
  diffStat?: string;
  commitsAhead: number;
  /** 提交或清理失败；文件与 branch 保留。 */
  error?: string;
}

export interface WorktreeIsolationPort {
  /** 在 cwd 所在仓库的 HEAD 上创建 worktree。失败抛 WorktreeIsolationError。 */
  create(input: { cwd: string; agentId: string }): Promise<WorktreeRecord>;
  /** worktree 缺失时从 branch（或 sourceHead）重建。失败抛 WorktreeIsolationError("unavailable")。 */
  ensure(record: WorktreeRecord): Promise<WorktreeRecord>;
  /** 提交未提交改动、无改动时清理。只在 child 真正结束后调用。 */
  finalize(record: WorktreeRecord, input: { message: string }): Promise<WorktreeFinalizeResult>;
}
