// git worktree 隔离的子代理（Fork）。合同见 docs/design/v2/tool/07-subagent.md「Worktree isolation
// 当前范围」。所有 git 调用都走 execFile（不经 shell），stdout 与 stderr 都保留以便报告。

import { execFile } from "node:child_process";
import { appendFile, mkdir, readFile, stat } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";
import {
  WorktreeIsolationError,
  type WorktreeFinalizeResult,
  type WorktreeIsolationPort,
  type WorktreeRecord,
} from "@zcode/contracts";

export const WORKTREES_DIR = join(".zcode", "worktrees");
const EXCLUDE_PATTERN = "/.zcode/worktrees/";
const GIT_TIMEOUT_MS = 60_000;
const FALLBACK_IDENTITY = ["-c", "user.name=ZCode", "-c", "user.email=zcode@localhost"];

export interface GitResult {
  code: number;
  stdout: string;
  stderr: string;
}

export type GitRunner = (args: string[], cwd: string) => Promise<GitResult>;

export const runGit: GitRunner = (args, cwd) =>
  new Promise((done) => {
    execFile(
      "git",
      args,
      {
        cwd,
        timeout: GIT_TIMEOUT_MS,
        windowsHide: true,
        maxBuffer: 16 * 1024 * 1024,
        // 收尾提交不能停下来等编辑器或凭据输入。
        env: { ...process.env, GIT_TERMINAL_PROMPT: "0", GIT_EDITOR: "true" },
      },
      (error, stdout, stderr) => {
        const code = error === null ? 0 : typeof error.code === "number" ? error.code : 1;
        done({ code, stdout: String(stdout ?? ""), stderr: String(stderr ?? "") });
      },
    );
  });

export interface GitWorktreeIsolationOptions {
  git?: GitRunner;
  env?: NodeJS.ProcessEnv;
}

export function createGitWorktreeIsolation(
  options: GitWorktreeIsolationOptions = {},
): WorktreeIsolationPort {
  const git = options.git ?? runGit;
  const env = options.env ?? process.env;

  async function must(args: string[], cwd: string, code: WorktreeIsolationError["code"]) {
    const result = await git(args, cwd);
    if (result.code !== 0) throw new WorktreeIsolationError(code, describe(args, result));
    return result.stdout.trim();
  }

  return {
    async create({ cwd, agentId }) {
      const top = await git(["rev-parse", "--show-toplevel"], cwd);
      if (top.code !== 0) {
        throw new WorktreeIsolationError(
          "not_a_repo",
          `isolation: "worktree" needs a git repository, and ${cwd} is not inside one`,
        );
      }
      const repoRoot = resolve(top.stdout.trim());
      const head = await git(["rev-parse", "--verify", "HEAD^{commit}"], repoRoot);
      if (head.code !== 0) {
        throw new WorktreeIsolationError(
          "no_commits",
          `isolation: "worktree" starts from HEAD, and ${repoRoot} has no commits yet`,
        );
      }
      const sourceHead = head.stdout.trim();
      await ensureExcluded(git, repoRoot);
      const path = join(repoRoot, WORKTREES_DIR, agentId);
      const branch = `zcode/${agentId}`;
      await mkdir(join(repoRoot, WORKTREES_DIR), { recursive: true });
      await must(["worktree", "add", "-b", branch, path, sourceHead], repoRoot, "create_failed");
      await must(["config", `branch.${branch}.zcodeBase`, sourceHead], repoRoot, "create_failed");
      const cargoTargetDir = await sharedCargoTargetDir(repoRoot, env);
      return {
        vcs: "git",
        repoRoot,
        path,
        branch,
        sourceHead,
        ...(cargoTargetDir ? { cargoTargetDir } : {}),
      };
    },

    async ensure(record) {
      if (await isWorktree(git, record.path)) {
        const { removed: _removed, ...rest } = record;
        return rest;
      }
      // 记录还在但目录没了：先清掉 git 里的陈旧登记，再从 branch（或 sourceHead）重建。
      await git(["worktree", "prune"], record.repoRoot);
      const branchExists =
        (await git(["rev-parse", "--verify", `refs/heads/${record.branch}`], record.repoRoot))
          .code === 0;
      const args = branchExists
        ? ["worktree", "add", record.path, record.branch]
        : ["worktree", "add", "-b", record.branch, record.path, record.sourceHead];
      const result = await git(args, record.repoRoot);
      if (result.code !== 0) {
        throw new WorktreeIsolationError(
          "unavailable",
          `isolation unavailable: the worktree ${record.path} is missing and could not be recreated (${describe(args, result)})`,
        );
      }
      if (!branchExists) {
        await git(
          ["config", `branch.${record.branch}.zcodeBase`, record.sourceHead],
          record.repoRoot,
        );
      }
      const { removed: _removed, ...rest } = record;
      return rest;
    },

    async finalize(record, { message }) {
      return finalizeWorktree(git, record, message);
    },
  };
}

async function finalizeWorktree(
  git: GitRunner,
  record: WorktreeRecord,
  message: string,
): Promise<WorktreeFinalizeResult> {
  const kept = (error: string, next: Partial<WorktreeRecord> = {}): WorktreeFinalizeResult => ({
    record: { ...record, ...next, finalizeError: error.slice(0, 2_000) },
    outcome: "kept",
    commitsAhead: 0,
    error,
  });
  if (!(await isWorktree(git, record.path))) {
    return kept(`the worktree ${record.path} is missing`);
  }
  const status = await git(["status", "--porcelain", "--untracked-files=all"], record.path);
  if (status.code !== 0) return kept(describe(["status"], status));

  let committed = false;
  if (status.stdout.trim() !== "") {
    const add = await git(["add", "-A"], record.path);
    if (add.code !== 0) return kept(describe(["add", "-A"], add));
    const identity =
      (await git(["var", "GIT_COMMITTER_IDENT"], record.path)).code === 0 ? [] : FALLBACK_IDENTITY;
    const commit = await git([...identity, "commit", "-q", "-m", message], record.path);
    if (commit.code !== 0) return kept(describe(["commit"], commit));
    committed = true;
  }

  const head = await git(["rev-parse", "HEAD"], record.path);
  if (head.code !== 0) return kept(describe(["rev-parse", "HEAD"], head));
  const resultHead = head.stdout.trim();
  const ahead = await git(
    ["rev-list", "--count", `${record.sourceHead}..${resultHead}`],
    record.path,
  );
  const commitsAhead = ahead.code === 0 ? Number.parseInt(ahead.stdout.trim(), 10) || 0 : 0;

  if (commitsAhead === 0 && !committed) {
    const remove = await git(["worktree", "remove", record.path], record.repoRoot);
    if (remove.code !== 0) return kept(describe(["worktree", "remove"], remove), { resultHead });
    const branch = await git(["branch", "-D", record.branch], record.repoRoot);
    if (branch.code !== 0) return kept(describe(["branch", "-D"], branch), { resultHead });
    const { finalizeError: _error, ...rest } = record;
    return { record: { ...rest, resultHead, removed: true }, outcome: "removed", commitsAhead: 0 };
  }

  const stat = await git(["diff", "--shortstat", record.sourceHead, resultHead], record.path);
  const { finalizeError: _error, ...rest } = record;
  return {
    record: { ...rest, resultHead },
    outcome: committed ? "committed" : "kept",
    commitsAhead,
    ...(stat.code === 0 && stat.stdout.trim() ? { diffStat: stat.stdout.trim() } : {}),
  };
}

/** `.zcode/worktrees/` 写进仓库的 exclude 文件（linked worktree 时由 git 指向公共目录）。 */
async function ensureExcluded(git: GitRunner, repoRoot: string): Promise<void> {
  const located = await git(["rev-parse", "--git-path", "info/exclude"], repoRoot);
  if (located.code !== 0) return;
  const raw = located.stdout.trim();
  await appendWorktreesExclude(isAbsolute(raw) ? raw : resolve(repoRoot, raw));
}

/** 幂等地把 `/.zcode/worktrees/` 追加到一个 git exclude 文件（jj 后端复用）。 */
export async function appendWorktreesExclude(excludePath: string): Promise<void> {
  const current = await readFile(excludePath, "utf8").catch(() => "");
  if (current.split(/\r?\n/u).some((line) => line.trim() === EXCLUDE_PATTERN)) return;
  await mkdir(resolve(excludePath, ".."), { recursive: true });
  const separator = current === "" || current.endsWith("\n") ? "" : "\n";
  await appendFile(excludePath, `${separator}${EXCLUDE_PATTERN}\n`);
}

async function isWorktree(git: GitRunner, path: string): Promise<boolean> {
  try {
    if (!(await stat(path)).isDirectory()) return false;
  } catch {
    return false;
  }
  const inside = await git(["rev-parse", "--is-inside-work-tree"], path);
  if (inside.code !== 0 || inside.stdout.trim() !== "true") return false;
  // 被删掉 .git 文件的目录会被父仓库当成普通子目录：确认顶层就是它自己。
  const top = await git(["rev-parse", "--show-toplevel"], path);
  return top.code === 0 && samePath(top.stdout.trim(), path);
}

/** 仓库根有 Cargo.toml 且用户没设 CARGO_TARGET_DIR 时，worktree 共享主 checkout 的 target。 */
export async function sharedCargoTargetDir(
  repoRoot: string,
  env: NodeJS.ProcessEnv,
): Promise<string | undefined> {
  return env.CARGO_TARGET_DIR === undefined && (await isFile(join(repoRoot, "Cargo.toml")))
    ? join(repoRoot, "target")
    : undefined;
}

async function isFile(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
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
  return `git ${args.join(" ")} failed (exit ${result.code})${output ? `: ${output}` : ""}`;
}
