import { describe, expect, it } from "vitest";
import type { WorktreeIsolationPort, WorktreeRecord } from "@zcode/contracts";
import { createAutoWorktreeIsolation, createJjDetector } from "../src/vcs/auto-worktree.js";

function fakePort(vcs: "git" | "jj", calls: string[]): WorktreeIsolationPort {
  const record = (agentId: string): WorktreeRecord => ({
    vcs,
    repoRoot: "/r",
    path: `/r/.zcode/worktrees/${agentId}`,
    branch: `zcode/${agentId}`,
    sourceHead: "abc",
  });
  return {
    async create({ agentId }) {
      calls.push(`${vcs}.create`);
      return record(agentId);
    },
    async ensure(r) {
      calls.push(`${vcs}.ensure`);
      return r;
    },
    async finalize(r) {
      calls.push(`${vcs}.finalize`);
      return { record: r, outcome: "kept", commitsAhead: 0 };
    },
  };
}

describe("auto worktree isolation", () => {
  it("creates with jj only where the detector says so", async () => {
    const calls: string[] = [];
    const port = createAutoWorktreeIsolation({
      git: fakePort("git", calls),
      jj: fakePort("jj", calls),
      detectJj: async (cwd) => cwd === "/jj",
    });
    expect((await port.create({ cwd: "/jj", agentId: "a" })).vcs).toBe("jj");
    expect((await port.create({ cwd: "/git", agentId: "b" })).vcs).toBe("git");
    expect(calls).toEqual(["jj.create", "git.create"]);
  });

  it("routes ensure and finalize by the record, treating records without vcs as git", async () => {
    const calls: string[] = [];
    const port = createAutoWorktreeIsolation({
      git: fakePort("git", calls),
      jj: fakePort("jj", calls),
      detectJj: async () => {
        throw new Error("existing records must not re-detect");
      },
    });
    const base = { repoRoot: "/r", path: "/r/x", branch: "zcode/x", sourceHead: "abc" };
    await port.ensure({ ...base, vcs: "jj" });
    await port.finalize({ ...base, vcs: "jj" }, { message: "m" });
    await port.ensure(base);
    await port.finalize(base, { message: "m" });
    expect(calls).toEqual(["jj.ensure", "jj.finalize", "git.ensure", "git.finalize"]);
  });

  it("probes jj --version once and jj root per cwd", async () => {
    const seen: string[] = [];
    const detect = createJjDetector(async (args, cwd) => {
      seen.push(`${args.join(" ")}@${cwd}`);
      return { code: args[0] === "root" && cwd === "/git" ? 1 : 0, stdout: "", stderr: "" };
    });
    expect(await detect("/jj")).toBe(true);
    expect(await detect("/git")).toBe(false);
    expect(seen).toEqual(["--version@/jj", "root@/jj", "root@/git"]);
  });

  it("never picks jj when the binary is missing", async () => {
    const seen: string[] = [];
    const detect = createJjDetector(async (args) => {
      seen.push(args.join(" "));
      return { code: 1, stdout: "", stderr: "not found" };
    });
    expect(await detect("/a")).toBe(false);
    expect(await detect("/b")).toBe(false);
    expect(seen).toEqual(["--version"]);
  });
});
