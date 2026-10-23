// jj workspace 隔离的子代理（Fork）。合同见 docs/design/v2/tool/07-subagent.md「jj 后端」。
// 所有 jj 调用都走 execFile（不经 shell）；只读查询带 --ignore-working-copy，不快照父 working copy。

import { execFile } from "node:child_process";
import { mkdir, rm, stat } from "node:fs/promises";
import { basename, isAbsolute, join, resolve } from "node:path";
import {
  WorktreeIsolationError,
  type WorktreeFinalizeResult,
  type WorktreeIsolationPort,
  type WorktreeRecord,
} from "@zcode/contracts";
import {
  appendWorktreesExclude,
  runGit,
  sharedCargoTargetDir,
  WORKTREES_DIR,
  type GitResult,
  type GitRunner,
} from "../git/worktree.js";

const JJ_TIMEOUT_MS = 60_000;
const IGNORE_WC = "--ignore-working-copy";
const FALLBACK_NAME = "ZCode";
const FALLBACK_EMAIL = "zcode@localhost";

export type JjRunner = (args: string[], cwd: string) => Promise<GitResult>;

export const runJj: JjRunner = (args, cwd) =>
  new Promise((done) => {
    execFile(
      "jj",
      ["--no-pager", "--color=never", ...args],
      { cwd, timeout: JJ_TIMEOUT_MS, windowsHide: true, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        const code = error === null ? 0 : typeof error.code === "number" ? error.code : 1;
        done({ code, stdout: String(stdout ?? ""), stderr: String(stderr ?? "") });
      },
    );
  });

export interface JjWorktreeIsolationOptions {
  jj?: JjRunner;
  /** 只用于清理 colocated 仓库里陈旧的 git worktree 登记。 */
  git?: GitRunner;
  env?: NodeJS.ProcessEnv;
}

export function jjWorkspaceName(agentId: string): string {
  return `zcode-${agentId}`;
}

export function createJjWorktreeIsolation(
  options: JjWorktreeIsolationOptions = {},
): WorktreeIsolationPort {
  const jj = options.jj ?? runJj;
  const git = options.git ?? runGit;
  const env = options.env ?? process.env;

  async function must(args: string[], cwd: string, code: WorktreeIsolationError["code"]) {
    const result = await jj(args, cwd);
    if (result.code !== 0) throw new WorktreeIsolationError(code, describe(args, result));
    return result.stdout.trim();
  }

  return {
    async create({ cwd, agentId }) {
      const root = await jj(["root"], cwd);
      if (root.code !== 0) {
        throw new WorktreeIsolationError(
          "not_a_repo",
          `isolation: "worktree" needs a jj or git repository, and ${cwd} is not inside one`,
        );
      }
      const repoRoot = resolve(root.stdout.trim());
      const parents = lines(
        await must(
          [
            IGNORE_WC,
            "log",
            "--no-graph",
            "-r",
            "@-",
            "-T",
            'if(root, "root", commit_id) ++ "\\n"',
          ],
          repoRoot,
          "create_failed",
        ),
      );
      if (parents.length === 0 || parents[0] === "root") {
        throw new WorktreeIsolationError(
          "no_commits",
          `isolation: "worktree" starts from the working copy's parent, and ${repoRoot} has no commits yet`,
        );
      }
      if (parents.length > 1) {
        throw new WorktreeIsolationError(
          "create_failed",
          `isolation: "worktree" starts from the working copy's parent, which is a merge of ${parents.length} commits in ${repoRoot}. Run \`jj new\` on a single commit first.`,
        );
      }
      const sourceHead = parents[0]!;
      await ensureExcluded(jj, repoRoot);
      const path = join(repoRoot, WORKTREES_DIR, agentId);
      await mkdir(join(repoRoot, WORKTREES_DIR), { recursive: true });
      // workspace add 拒绝 --ignore-working-copy（jj 0.46），这里会快照一次父 working copy。
      await must(
        ["workspace", "add", "--name", jjWorkspaceName(agentId), "-r", sourceHead, path],
        repoRoot,
        "create_failed",
      );
      await must(
        ["config", "set", "--repo", baseConfigKey(agentId), sourceHead],
        repoRoot,
        "create_failed",
      );
      const cargoTargetDir = await sharedCargoTargetDir(repoRoot, env);
      return {
        vcs: "jj",
        repoRoot,
        path,
        branch: `zcode/${agentId}`,
        sourceHead,
        // 非 colocated 仓库的 workspace 没有 .git：阻止 child 里的 git 向上找到外层仓库。
        env: { GIT_CEILING_DIRECTORIES: join(repoRoot, WORKTREES_DIR) },
        ...(cargoTargetDir ? { cargoTargetDir } : {}),
      };
    },

    async ensure(record) {
      const agentId = basename(record.path);
      if (await isWorkspace(jj, record.path)) {
        // 父集成（rebase）后 child 会 stale；不 stale 时这是 no-op。
        const update = await jj(["workspace", "update-stale"], record.path);
        if (update.code !== 0) {
          throw new WorktreeIsolationError(
            "unavailable",
            `isolation unavailable: the jj workspace ${record.path} could not be updated (${describe(["workspace", "update-stale"], update)})`,
          );
        }
        return stripRemoved(record);
      }
      // 记录还在但目录没了：先忘掉陈旧的 workspace，再从 bookmark（或 sourceHead）重建。
      await jj([IGNORE_WC, "workspace", "forget", jjWorkspaceName(agentId)], record.repoRoot);
      // colocated 仓库里目录被删后 git 仍登记着这个 worktree，jj workspace add 会被 git 拒绝；
      // 只清理 jj 底层 git 目录里指向缺失目录的登记。
      const gitDir = await jjGitDir(jj, record.repoRoot);
      if (gitDir) await git(["--git-dir", gitDir, "worktree", "prune"], record.repoRoot);
      const bookmark = await jj(
        [
          IGNORE_WC,
          "log",
          "--no-graph",
          "-r",
          `present(${quoteRev(record.branch)})`,
          "-T",
          "commit_id",
        ],
        record.repoRoot,
      );
      const from =
        bookmark.code === 0 && bookmark.stdout.trim() ? bookmark.stdout.trim() : record.sourceHead;
      await mkdir(join(record.repoRoot, WORKTREES_DIR), { recursive: true });
      const args = [
        "workspace",
        "add",
        "--name",
        jjWorkspaceName(agentId),
        "-r",
        from,
        record.path,
      ];
      const result = await jj(args, record.repoRoot);
      if (result.code !== 0) {
        throw new WorktreeIsolationError(
          "unavailable",
          `isolation unavailable: the jj workspace ${record.path} is missing and could not be recreated (${describe(args, result)})`,
        );
      }
      return stripRemoved(record);
    },

    async finalize(record, { message }) {
      return finalizeWorkspace(jj, record, message);
    },
  };
}

async function finalizeWorkspace(
  jj: JjRunner,
  record: WorktreeRecord,
  message: string,
): Promise<WorktreeFinalizeResult> {
  const kept = (error: string, next: Partial<WorktreeRecord> = {}): WorktreeFinalizeResult => ({
    record: { ...record, ...next, finalizeError: error.slice(0, 2_000) },
    outcome: "kept",
    commitsAhead: 0,
    error,
  });
  const agentId = basename(record.path);
  if (!(await isWorkspace(jj, record.path))) {
    return kept(`the jj workspace ${record.path} is missing`);
  }
  await jj(["workspace", "update-stale"], record.path);
  // 不带 --ignore-working-copy：这一次快照 child 的 working copy。
  const stateArgs = [
    "log",
    "--no-graph",
    "-r",
    "@",
    "-T",
    'if(empty,"1","0") ++ if(description,"1","0")',
  ];
  const state = await jj(stateArgs, record.path);
  if (state.code !== 0) return kept(describe(stateArgs, state));
  const [isEmpty, hasDescription] = [
    state.stdout.trim()[0] === "1",
    state.stdout.trim()[1] === "1",
  ];

  let committed = false;
  if (!isEmpty && !hasDescription) {
    const identity = await identityArgs(jj, record.path);
    const args = [...identity, "describe", "-m", message];
    const described = await jj(args, record.path);
    if (described.code !== 0) return kept(describe(["describe"], described));
    committed = true;
  }

  let resultRev = isEmpty && !hasDescription ? "@-" : "@";
  let heads = await jj(
    [IGNORE_WC, "log", "--no-graph", "-r", resultRev, "-T", 'commit_id ++ "\\n"'],
    record.path,
  );
  if (heads.code === 0 && lines(heads.stdout).length > 1) {
    // agent 把 working copy 放在 merge 之上：merge 本身才是结果。
    resultRev = "@";
    heads = await jj([IGNORE_WC, "log", "--no-graph", "-r", "@", "-T", "commit_id"], record.path);
  }
  if (heads.code !== 0) return kept(describe(["log", "-r", resultRev], heads));
  const resultHead = lines(heads.stdout)[0] ?? "";
  const ahead = await jj(
    [
      IGNORE_WC,
      "log",
      "--no-graph",
      "-r",
      `${record.sourceHead}..${resultHead}`,
      "-T",
      'commit_id ++ "\\n"',
    ],
    record.path,
  );
  // 数不出来就保留：不能因为一次查询失败把 agent 的工作当成「没有改动」删掉。
  if (ahead.code !== 0)
    return kept(describe(["log", "-r", "sourceHead..result"], ahead), { resultHead });
  const commitsAhead = lines(ahead.stdout).length;

  if (commitsAhead === 0 && !committed) {
    const forget = await jj(
      [IGNORE_WC, "workspace", "forget", jjWorkspaceName(agentId)],
      record.repoRoot,
    );
    if (forget.code !== 0) return kept(describe(["workspace", "forget"], forget), { resultHead });
    try {
      await rm(record.path, { recursive: true, force: true });
    } catch (error) {
      return kept(`removing ${record.path} failed: ${String(error)}`, { resultHead });
    }
    const deleteArgs = [IGNORE_WC, "bookmark", "delete", record.branch];
    const deleted = await jj(deleteArgs, record.repoRoot);
    if (deleted.code !== 0) return kept(describe(deleteArgs, deleted), { resultHead });
    // 键可能从未写入（旧记录）；缺失时 jj 返回非零，忽略即可。
    await jj(["config", "unset", "--repo", baseConfigKey(agentId)], record.repoRoot);
    const { finalizeError: _error, ...rest } = record;
    return { record: { ...rest, resultHead, removed: true }, outcome: "removed", commitsAhead: 0 };
  }

  const setArgs = [
    IGNORE_WC,
    "bookmark",
    "set",
    record.branch,
    "-r",
    resultHead,
    "--allow-backwards",
  ];
  const set = await jj(setArgs, record.repoRoot);
  if (set.code !== 0) return kept(describe(setArgs, set), { resultHead });
  if (resultRev === "@") {
    // 结果就是 child 的 working copy：换一个新的空 @，resume 后的编辑才不会改写父已集成的提交。
    const identity = await identityArgs(jj, record.path);
    const fresh = await jj([...identity, "new"], record.path);
    if (fresh.code !== 0) return kept(describe(["new"], fresh), { resultHead });
  }
  const stat = await jj(
    [IGNORE_WC, "diff", "--stat", "--from", record.sourceHead, "--to", resultHead],
    record.path,
  );
  const diffStat = stat.code === 0 ? lines(stat.stdout).at(-1) : undefined;
  const { finalizeError: _error, ...rest } = record;
  return {
    record: { ...rest, resultHead },
    outcome: committed ? "committed" : "kept",
    commitsAhead,
    ...(diffStat ? { diffStat } : {}),
  };
}

/** `/.zcode/worktrees/` 写进 jj 底层 git 目录的 exclude（colocated 时父 `git status` 才不显示它）。 */
async function ensureExcluded(jj: JjRunner, repoRoot: string): Promise<void> {
  const gitDir = await jjGitDir(jj, repoRoot);
  if (gitDir) await appendWorktreesExclude(join(gitDir, "info", "exclude"));
}

/** jj 底层 git 仓库的目录；不是 git 后端时为 undefined。 */
async function jjGitDir(jj: JjRunner, repoRoot: string): Promise<string | undefined> {
  const located = await jj([IGNORE_WC, "git", "root"], repoRoot);
  if (located.code !== 0) return undefined;
  const raw = located.stdout.trim();
  return isAbsolute(raw) ? raw : resolve(repoRoot, raw);
}

async function isWorkspace(jj: JjRunner, path: string): Promise<boolean> {
  try {
    if (!(await stat(join(path, ".jj"))).isDirectory()) return false;
  } catch {
    return false;
  }
  const root = await jj(["root"], path);
  return root.code === 0 && samePath(root.stdout.trim(), path);
}

/** 只在仓库没有配置身份时补上 ZCode 回退身份（未配置时 `jj config get` 输出为空）。 */
async function identityArgs(jj: JjRunner, cwd: string): Promise<string[]> {
  const args: string[] = [];
  const name = await jj(["config", "get", "user.name"], cwd);
  if (name.code !== 0 || !name.stdout.trim()) args.push("--config", `user.name=${FALLBACK_NAME}`);
  const email = await jj(["config", "get", "user.email"], cwd);
  if (email.code !== 0 || !email.stdout.trim()) {
    args.push("--config", `user.email=${FALLBACK_EMAIL}`);
  }
  return args;
}

function baseConfigKey(agentId: string): string {
  return `zcode.base.${agentId}`;
}

/** bookmark 名作为 revset 符号时要加引号（`zcode/agent_x` 里的 `/` 与 `-` 是运算符）。 */
function quoteRev(name: string): string {
  return `"${name.replace(/["\\]/gu, "\\$&")}"`;
}

function stripRemoved(record: WorktreeRecord): WorktreeRecord {
  const { removed: _removed, ...rest } = record;
  return rest;
}

function lines(text: string): string[] {
  return text
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean);
}

function samePath(left: string, right: string): boolean {
  const normalize = (value: string) => {
    const resolved = resolve(value).replace(/\\/gu, "/");
    return process.platform === "win32" ? resolved.toLowerCase() : resolved;
  };
  return normalize(left) === normalize(right);
}

function describe(args: string[], result: GitResult): string {
  const output = (result.stderr.trim() || result.stdout.trim())
    .split(/\r?\n/u)
    .slice(0, 6)
    .join("\n");
  return `jj ${args.join(" ")} failed (exit ${result.code})${output ? `: ${output}` : ""}`;
}
