// 按仓库选择 git worktree 或 jj workspace 后端（Fork）。合同见 docs/design/v2/tool/07-subagent.md「jj 后端」。

import { worktreeVcs, type WorktreeIsolationPort, type WorktreeRecord } from "@zcode/contracts";
import { createGitWorktreeIsolation } from "../git/worktree.js";
import { createJjWorktreeIsolation, runJj, type JjRunner } from "../jj/workspace.js";

export interface AutoWorktreeIsolationOptions {
  git?: WorktreeIsolationPort;
  jj?: WorktreeIsolationPort;
  /** cwd 在 jj 仓库里且 `jj` 可执行。 */
  detectJj?: (cwd: string) => Promise<boolean>;
}

export function createAutoWorktreeIsolation(
  options: AutoWorktreeIsolationOptions = {},
): WorktreeIsolationPort {
  const git = options.git ?? createGitWorktreeIsolation();
  const jj = options.jj ?? createJjWorktreeIsolation();
  const detectJj = options.detectJj ?? createJjDetector();
  // 已创建的记录按自己的 vcs 收尾与恢复，不再重新探测（没有 vcs 的旧记录是 git）。
  const owner = (record: WorktreeRecord) => (worktreeVcs(record) === "jj" ? jj : git);
  return {
    async create(input) {
      return ((await detectJj(input.cwd)) ? jj : git).create(input);
    },
    ensure: (record) => owner(record).ensure(record),
    finalize: (record, input) => owner(record).finalize(record, input),
  };
}

/** `jj --version` 每个探测器只跑一次；仓库判断每次按 cwd 重新做。 */
export function createJjDetector(jj: JjRunner = runJj): (cwd: string) => Promise<boolean> {
  let available: Promise<boolean> | undefined;
  return async (cwd) => {
    available ??= jj(["--version"], cwd).then((result) => result.code === 0);
    if (!(await available)) return false;
    return (await jj(["root"], cwd)).code === 0;
  };
}
