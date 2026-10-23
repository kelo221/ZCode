import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { createJjWorktreeIsolation } from "../src/jj/workspace.js";

const hasJj = spawnSync("jj", ["--version"], { windowsHide: true }).status === 0;
const dirs: string[] = [];
let savedJjConfig: string | undefined;

function jj(cwd: string, ...args: string[]): string {
  return execFileSync("jj", ["--no-pager", "--color=never", ...args], {
    cwd,
    encoding: "utf8",
    windowsHide: true,
  }).trim();
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf8", windowsHide: true }).trim();
}

function rev(cwd: string, revset: string): string {
  return jj(
    cwd,
    "--ignore-working-copy",
    "log",
    "--no-graph",
    "-r",
    revset,
    "-T",
    'commit_id ++ "\\n"',
  );
}

async function tempDir(): Promise<string> {
  const dir = await realpath(await mkdtemp(join(tmpdir(), "zc-jj-")));
  dirs.push(dir);
  return dir;
}

/** git 仓库 + 一次提交，再 colocate 成 jj 仓库。 */
async function colocatedRepo(): Promise<string> {
  const dir = await tempDir();
  git(dir, "init", "-q", "-b", "main");
  git(dir, "config", "user.name", "Test");
  git(dir, "config", "user.email", "test@example.com");
  git(dir, "config", "commit.gpgsign", "false");
  git(dir, "config", "core.autocrlf", "false");
  await writeFile(join(dir, "a.txt"), "one\n");
  git(dir, "add", "a.txt");
  git(dir, "commit", "-q", "-m", "init");
  jj(dir, "git", "init", "--colocate");
  return dir;
}

async function plainJjRepo(): Promise<string> {
  const dir = await tempDir();
  jj(dir, "git", "init", "--no-colocate");
  await writeFile(join(dir, "a.txt"), "one\n");
  jj(dir, "commit", "-m", "init");
  return dir;
}

beforeAll(async () => {
  // 测试不能读用户自己的 jj 配置（身份、签名、auto-track 等）。
  savedJjConfig = process.env.JJ_CONFIG;
  const config = join(await tempDir(), "jj-config.toml");
  await writeFile(config, '[user]\nname = "Test"\nemail = "test@example.com"\n');
  process.env.JJ_CONFIG = config;
});

afterAll(() => {
  if (savedJjConfig === undefined) delete process.env.JJ_CONFIG;
  else process.env.JJ_CONFIG = savedJjConfig;
});

afterEach(async () => {
  // beforeAll 的配置目录排在第一位，留到 afterAll 之后。
  for (const dir of dirs.splice(1)) {
    await rm(dir, { recursive: true, force: true, maxRetries: 3 });
  }
});

describe.skipIf(!hasJj)("jj workspace isolation", () => {
  it("creates a workspace on the parent's @- and keeps .zcode/worktrees out of the parent", async () => {
    const repo = await colocatedRepo();
    await writeFile(join(repo, "a.txt"), "uncommitted parent edit\n");
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_1" });

    expect(record.vcs).toBe("jj");
    expect(record.path).toBe(join(repo, ".zcode", "worktrees", "agent_1"));
    expect(record.branch).toBe("zcode/agent_1");
    expect(record.sourceHead).toBe(git(repo, "rev-parse", "main"));
    expect(record.env).toEqual({ GIT_CEILING_DIRECTORIES: join(repo, ".zcode", "worktrees") });
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("one\n");
    expect(jj(repo, "config", "get", "zcode.base.agent_1")).toBe(record.sourceHead);
    expect(git(repo, "status", "--porcelain")).not.toContain(".zcode");
    expect(jj(repo, "status")).not.toContain(".zcode");

    await port.create({ cwd: repo, agentId: "agent_2" });
    const exclude = await readFile(join(repo, ".git", "info", "exclude"), "utf8");
    expect(exclude.split(/\r?\n/u).filter((line) => line === "/.zcode/worktrees/")).toHaveLength(1);
  });

  it("blocks git discovery inside a non-colocated workspace", async () => {
    const repo = await plainJjRepo();
    const record = await createJjWorktreeIsolation({ env: {} }).create({
      cwd: repo,
      agentId: "agent_g",
    });
    expect(existsSync(join(record.path, ".git"))).toBe(false);
    const probe = spawnSync("git", ["rev-parse", "--show-toplevel"], {
      cwd: record.path,
      env: { ...process.env, ...record.env },
      windowsHide: true,
    });
    expect(probe.status).not.toBe(0);
    await writeFile(join(record.path, "child.txt"), "child\n");
    expect(jj(repo, "status")).not.toContain("child.txt");
  });

  it("refuses outside a repository, without commits, and on a merge parent", async () => {
    const port = createJjWorktreeIsolation({ env: {} });
    await expect(port.create({ cwd: await tempDir(), agentId: "x" })).rejects.toMatchObject({
      code: "not_a_repo",
    });
    const empty = await tempDir();
    jj(empty, "git", "init");
    await expect(port.create({ cwd: empty, agentId: "x" })).rejects.toMatchObject({
      code: "no_commits",
    });
    const repo = await plainJjRepo();
    jj(repo, "new", "root()");
    await writeFile(join(repo, "b.txt"), "b\n");
    jj(repo, "commit", "-m", "side");
    jj(repo, "new", "description(substring:init)", "description(substring:side)");
    await expect(port.create({ cwd: repo, agentId: "x" })).rejects.toMatchObject({
      code: "create_failed",
    });
  });

  it("forgets and removes the workspace when the agent changed nothing", async () => {
    const repo = await colocatedRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_n" });
    const result = await port.finalize(record, { message: "zcode agent agent_n: nothing" });

    expect(result.outcome).toBe("removed");
    expect(result.record.removed).toBe(true);
    expect(existsSync(record.path)).toBe(false);
    expect(jj(repo, "workspace", "list")).not.toContain("zcode-agent_n");
    expect(rev(repo, 'present("zcode/agent_n")')).toBe("");
    expect(git(repo, "worktree", "list")).not.toContain("agent_n");
  });

  it("describes uncommitted work, sets the bookmark and starts a fresh working copy", async () => {
    const repo = await colocatedRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_c" });
    await writeFile(join(record.path, "a.txt"), "two\n");
    await writeFile(join(record.path, "new.txt"), "new\n");
    const result = await port.finalize(record, { message: "zcode agent agent_c: edit" });

    expect(result.outcome).toBe("committed");
    expect(result.commitsAhead).toBe(1);
    expect(result.diffStat).toMatch(/2 files changed/u);
    expect(result.record.resultHead).toBe(rev(repo, "zcode/agent_c"));
    expect(git(repo, "log", "-1", "--format=%s", "zcode/agent_c")).toBe(
      "zcode agent agent_c: edit",
    );
    expect(rev(record.path, "@-")).toBe(result.record.resultHead);
    expect(existsSync(record.path)).toBe(true);
  });

  it("keeps an agent-made commit without describing anything", async () => {
    const repo = await plainJjRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_k" });
    await writeFile(join(record.path, "a.txt"), "agent\n");
    jj(record.path, "commit", "-m", "agent commit");
    const agentHead = rev(record.path, "@-");
    const result = await port.finalize(record, { message: "unused" });

    expect(result.outcome).toBe("kept");
    expect(result.commitsAhead).toBe(1);
    expect(result.record.resultHead).toBe(agentHead);
    expect(rev(repo, "zcode/agent_k")).toBe(agentHead);
  });

  it("integrates with the two-rebase command and leaves a single @- for the next agent", async () => {
    const repo = await plainJjRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_i" });
    await writeFile(join(record.path, "agent.txt"), "agent\n");
    await port.finalize(record, { message: "zcode agent agent_i: work" });
    await writeFile(join(repo, "p.txt"), "parent\n");
    jj(repo, "commit", "-m", "parent work");
    await writeFile(join(repo, "dirty.txt"), "dirty\n");

    jj(repo, "rebase", "-b", record.branch, "-d", "@-");
    jj(repo, "rebase", "-s", "@", "-d", record.branch);
    expect(await readFile(join(repo, "agent.txt"), "utf8")).toBe("agent\n");
    expect(await readFile(join(repo, "dirty.txt"), "utf8")).toBe("dirty\n");
    expect(rev(repo, "@-").split("\n")).toHaveLength(1);
    await expect(port.create({ cwd: repo, agentId: "agent_next" })).resolves.toMatchObject({
      sourceHead: rev(repo, record.branch),
    });
    // 被集成后 child 是 stale 的，ensure 负责更新。
    await expect(port.ensure(record)).resolves.toMatchObject({ path: record.path });
    expect(jj(record.path, "status")).toContain("Working copy");
  });

  it("recreates a missing workspace from its bookmark, then from sourceHead", async () => {
    const repo = await colocatedRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_r" });
    await writeFile(join(record.path, "a.txt"), "on bookmark\n");
    const finalized = await port.finalize(record, { message: "zcode agent agent_r: work" });
    await rm(record.path, { recursive: true, force: true });

    const fromBookmark = await port.ensure(finalized.record);
    expect(fromBookmark.path).toBe(record.path);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("on bookmark\n");

    jj(repo, "workspace", "forget", "zcode-agent_r");
    await rm(record.path, { recursive: true, force: true });
    jj(repo, "bookmark", "delete", record.branch);
    const fromHead = await port.ensure(finalized.record);
    expect(fromHead.branch).toBe(record.branch);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("one\n");
  });

  it("reports isolation unavailable when nothing can be recreated", async () => {
    const repo = await plainJjRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_u" });
    jj(repo, "workspace", "forget", "zcode-agent_u");
    await rm(record.path, { recursive: true, force: true });
    const broken = { ...record, sourceHead: "f".repeat(40) };
    await expect(port.ensure(broken)).rejects.toMatchObject({ code: "unavailable" });
  });

  it("leaves an existing workspace's files alone on ensure", async () => {
    const repo = await plainJjRepo();
    const port = createJjWorktreeIsolation({ env: {} });
    const record = await port.create({ cwd: repo, agentId: "agent_e" });
    await writeFile(join(record.path, "a.txt"), "dirty\n");
    expect(await port.ensure(record)).toEqual(record);
    expect(await readFile(join(record.path, "a.txt"), "utf8")).toBe("dirty\n");
  });
});
