import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { WorktreeIsolationError } from "@zcode/contracts";
import { createGitWorktreeIsolation } from "../src/git/worktree.js";

const dirs: string[] = [];

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf8", windowsHide: true }).trim();
}

async function tempDir(): Promise<string> {
  const dir = await realpath(await mkdtemp(join(tmpdir(), "zc-wt-")));
  dirs.push(dir);
  return dir;
}

async function repoWithCommit(): Promise<string> {
  const dir = await tempDir();
  git(dir, "init", "-q", "-b", "main");
  git(dir, "config", "user.name", "Test");
  git(dir, "config", "user.email", "test@example.com");
  git(dir, "config", "commit.gpgsign", "false");
  git(dir, "config", "core.autocrlf", "false");
  await writeFile(join(dir, "a.txt"), "one\n");
  git(dir, "add", "a.txt");
  git(dir, "commit", "-q", "-m", "init");
  return dir;
}

afterEach(async () => {
  for (const dir of dirs.splice(0)) {
    await rm(dir, { recursive: true, force: true, maxRetries: 3 });
  }
});

describe("git worktree isolation", () => {
  it("creates a worktree on zcode/<id> from HEAD and excludes .zcode/worktrees", async () => {
    const repo = await repoWithCommit();
    await writeFile(join(repo, "a.txt"), "uncommitted parent edit\n");
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_1" });

    expect(record.path).toBe(join(repo, ".zcode", "worktrees", "agent_1"));
    expect(record.branch).toBe("zcode/agent_1");
    expect(record.sourceHead).toBe(git(repo, "rev-parse", "HEAD"));
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("one\n");
    expect(git(repo, "config", "branch.zcode/agent_1.zcodeBase")).toBe(record.sourceHead);
    expect(git(repo, "status", "--porcelain")).toBe("M a.txt");
    const exclude = await readFile(join(repo, ".git", "info", "exclude"), "utf8");
    expect(exclude.split(/\r?\n/u).filter((line) => line === "/.zcode/worktrees/")).toHaveLength(1);

    await port.create({ cwd: repo, agentId: "agent_2" });
    const again = await readFile(join(repo, ".git", "info", "exclude"), "utf8");
    expect(again.split(/\r?\n/u).filter((line) => line === "/.zcode/worktrees/")).toHaveLength(1);
  });

  it("shares the Cargo target dir only when CARGO_TARGET_DIR is unset", async () => {
    const repo = await repoWithCommit();
    await writeFile(join(repo, "Cargo.toml"), "[workspace]\n");
    git(repo, "add", "Cargo.toml");
    git(repo, "commit", "-q", "-m", "cargo");
    const shared = await createGitWorktreeIsolation({ env: {} }).create({
      cwd: repo,
      agentId: "a",
    });
    expect(shared.cargoTargetDir).toBe(join(repo, "target"));
    const explicit = await createGitWorktreeIsolation({
      env: { CARGO_TARGET_DIR: "D:\\t" },
    }).create({
      cwd: repo,
      agentId: "b",
    });
    expect(explicit.cargoTargetDir).toBeUndefined();
  });

  it("finds the common exclude file when launched from a linked worktree", async () => {
    const repo = await repoWithCommit();
    const linked = join(await tempDir(), "linked");
    git(repo, "worktree", "add", "-q", "-b", "side", linked);
    const record = await createGitWorktreeIsolation({ env: {} }).create({
      cwd: linked,
      agentId: "agent_l",
    });
    expect(record.repoRoot).toBe(resolve(linked));
    expect(record.path).toBe(join(linked, ".zcode", "worktrees", "agent_l"));
    const exclude = await readFile(join(repo, ".git", "info", "exclude"), "utf8");
    expect(exclude).toContain("/.zcode/worktrees/");
    expect(git(linked, "status", "--porcelain")).toBe("");
  });

  it("refuses outside a repository and in a repository without commits", async () => {
    const port = createGitWorktreeIsolation({ env: {} });
    const plain = await tempDir();
    await expect(port.create({ cwd: plain, agentId: "x" })).rejects.toMatchObject({
      code: "not_a_repo",
    });
    const empty = await tempDir();
    git(empty, "init", "-q");
    const error = await port.create({ cwd: empty, agentId: "x" }).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(WorktreeIsolationError);
    expect((error as WorktreeIsolationError).code).toBe("no_commits");
  });

  it("removes the worktree and branch when the agent changed nothing", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_n" });
    const result = await port.finalize(record, { message: "zcode agent agent_n: nothing" });
    expect(result.outcome).toBe("removed");
    expect(result.record.removed).toBe(true);
    expect(existsSync(record.path)).toBe(false);
    expect(git(repo, "branch", "--list", "zcode/agent_n")).toBe("");
  });

  it("commits uncommitted work, including untracked files", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_c" });
    await writeFile(join(record.path, "a.txt"), "two\n");
    await writeFile(join(record.path, "new.txt"), "new\n");
    const result = await port.finalize(record, { message: "zcode agent agent_c: edit" });

    expect(result.outcome).toBe("committed");
    expect(result.commitsAhead).toBe(1);
    expect(result.diffStat).toMatch(/2 files changed/u);
    expect(result.record.resultHead).toBe(git(repo, "rev-parse", "zcode/agent_c"));
    expect(git(repo, "log", "-1", "--format=%s", "zcode/agent_c")).toBe(
      "zcode agent agent_c: edit",
    );
    expect(existsSync(record.path)).toBe(true);
    expect(git(repo, "status", "--porcelain")).toBe("");
  });

  it("keeps an agent-made commit without adding an empty one", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_k" });
    await writeFile(join(record.path, "a.txt"), "agent\n");
    git(record.path, "commit", "-q", "-am", "agent commit");
    const agentHead = git(record.path, "rev-parse", "HEAD");
    const result = await port.finalize(record, { message: "unused" });

    expect(result.outcome).toBe("kept");
    expect(result.commitsAhead).toBe(1);
    expect(result.record.resultHead).toBe(agentHead);
    expect(git(repo, "rev-list", "--count", `${record.sourceHead}..zcode/agent_k`)).toBe("1");
  });

  it("keeps files and records the error when a commit hook rejects", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_h" });
    await writeFile(
      join(repo, ".git", "hooks", "pre-commit"),
      "#!/bin/sh\necho hook says no >&2\nexit 1\n",
    );
    await writeFile(join(record.path, "a.txt"), "blocked\n");
    const result = await port.finalize(record, { message: "zcode agent agent_h: blocked" });

    expect(result.outcome).toBe("kept");
    expect(result.error).toMatch(/hook says no/u);
    expect(result.record.finalizeError).toMatch(/hook says no/u);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("blocked\n");
    expect(git(repo, "rev-parse", "zcode/agent_h")).toBe(record.sourceHead);

    await rm(join(repo, ".git", "hooks", "pre-commit"));
    const retried = await port.finalize(result.record, { message: "zcode agent agent_h: retry" });
    expect(retried.outcome).toBe("committed");
    expect(retried.record.finalizeError).toBeUndefined();
  });

  it("recreates a missing worktree from its branch, then from sourceHead", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_r" });
    await writeFile(join(record.path, "a.txt"), "on branch\n");
    const finalized = await port.finalize(record, { message: "zcode agent agent_r: work" });
    await rm(record.path, { recursive: true, force: true });

    const fromBranch = await port.ensure(finalized.record);
    expect(fromBranch.path).toBe(record.path);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("on branch\n");

    git(repo, "worktree", "remove", "--force", record.path);
    git(repo, "branch", "-D", record.branch);
    const fromHead = await port.ensure(finalized.record);
    expect(fromHead.branch).toBe(record.branch);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("one\n");
  });

  it("reports isolation unavailable when nothing can be recreated", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_u" });
    git(repo, "worktree", "remove", "--force", record.path);
    git(repo, "branch", "-D", record.branch);
    const broken = { ...record, sourceHead: "0".repeat(40) };
    await expect(port.ensure(broken)).rejects.toMatchObject({ code: "unavailable" });
    expect(existsSync(record.path)).toBe(false);
  });

  it("leaves an existing worktree alone on ensure", async () => {
    const repo = await repoWithCommit();
    const port = createGitWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_e" });
    await writeFile(join(record.path, "a.txt"), "dirty\n");
    expect(await port.ensure(record)).toEqual(record);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("dirty\n");
  });
});
