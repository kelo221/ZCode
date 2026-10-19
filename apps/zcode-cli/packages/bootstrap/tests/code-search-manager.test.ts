import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CodeSearchEngine, CodeSearchEngineLoad } from "@zcode/adapters";
import { CodeSearchManager } from "../src/zcode-protocol/code-search-manager.js";

const FIND = { query: "x", maxResults: 5 };

function fakeEngine(): CodeSearchEngine & { destroyed: boolean } {
  const engine = {
    destroyed: false,
    find: vi.fn(() => ({ ok: true as const, files: [{ path: "a.ts" }], totalMatched: 1 })),
    search: vi.fn(() => ({
      ok: true as const,
      matches: [],
      mode: "plain" as const,
      fuzzyFallback: false,
    })),
    destroy: vi.fn(() => {
      engine.destroyed = true;
    }),
  };
  return engine;
}

function setup(options: { busy?: (key: string) => boolean; load?: CodeSearchEngineLoad } = {}) {
  const engines: Array<ReturnType<typeof fakeEngine>> = [];
  const open = vi.fn(async (_root: string) => {
    const engine = fakeEngine();
    engines.push(engine);
    return engine;
  });
  const busy = vi.fn(options.busy ?? (() => false));
  const manager = new CodeSearchManager({
    load: async () => options.load ?? { ok: true, open },
    isWorkspaceBusy: busy,
  });
  return { manager, engines, open, busy };
}

describe("CodeSearchManager", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("opens lazily, once, even with concurrent first queries", async () => {
    const { manager, open, engines } = setup();
    expect(await manager.available()).toBe(true);
    expect(open).not.toHaveBeenCalled();
    const port = manager.portFor({ workspaceKey: "w", root: "/w" });
    const [a, b] = await Promise.all([port.findFiles(FIND), port.findFiles(FIND)]);
    expect(a).toMatchObject({ ok: true, files: [{ path: "a.ts" }] });
    expect(b.ok).toBe(true);
    expect(open).toHaveBeenCalledTimes(1);
    expect(open).toHaveBeenCalledWith("/w");
    expect(engines[0]?.find).toHaveBeenCalledTimes(2);
  });

  it("closes an idle workspace one minute after its last agent stops, then reopens on demand", async () => {
    const { manager, engines, open } = setup();
    await manager.available();
    const port = manager.portFor({ workspaceKey: "w", root: "/w" });
    await port.findFiles(FIND);
    await vi.advanceTimersByTimeAsync(55_000);
    expect(engines[0]?.destroyed).toBe(false);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(engines[0]?.destroyed).toBe(true);
    expect(manager.openWorkspaceKeys()).toEqual([]);

    await port.findFiles(FIND);
    expect(open).toHaveBeenCalledTimes(2);
    expect(manager.openWorkspaceKeys()).toEqual(["w"]);
  });

  it("keeps the index while any agent in the workspace is busy and restarts the countdown after", async () => {
    let busy = true;
    const { manager, engines } = setup({ busy: () => busy });
    await manager.available();
    await manager.portFor({ workspaceKey: "w", root: "/w" }).findFiles(FIND);
    await vi.advanceTimersByTimeAsync(10 * 60_000);
    expect(engines[0]?.destroyed).toBe(false);
    busy = false;
    await vi.advanceTimersByTimeAsync(55_000);
    expect(engines[0]?.destroyed).toBe(false);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(engines[0]?.destroyed).toBe(true);
  });

  it("activity during the countdown resets it", async () => {
    const { manager, engines } = setup();
    await manager.available();
    const port = manager.portFor({ workspaceKey: "w", root: "/w" });
    await port.findFiles(FIND);
    await vi.advanceTimersByTimeAsync(50_000);
    await port.searchCode({ query: "x", outputMode: "content", context: 0, maxResults: 5 });
    await vi.advanceTimersByTimeAsync(50_000);
    expect(engines[0]?.destroyed).toBe(false);
    await vi.advanceTimersByTimeAsync(15_000);
    expect(engines[0]?.destroyed).toBe(true);
  });

  it("a failing busy probe counts as busy", async () => {
    const { manager, engines } = setup({
      busy: () => {
        throw new Error("probe failed");
      },
    });
    await manager.available();
    await manager.portFor({ workspaceKey: "w", root: "/w" }).findFiles(FIND);
    await vi.advanceTimersByTimeAsync(5 * 60_000);
    expect(engines[0]?.destroyed).toBe(false);
  });

  it("tracks workspaces independently", async () => {
    const { manager, engines } = setup({ busy: (key) => key === "a" });
    await manager.available();
    await manager.portFor({ workspaceKey: "a", root: "/a" }).findFiles(FIND);
    await manager.portFor({ workspaceKey: "b", root: "/b" }).findFiles(FIND);
    await vi.advanceTimersByTimeAsync(70_000);
    expect(engines[0]?.destroyed).toBe(false);
    expect(engines[1]?.destroyed).toBe(true);
    expect(manager.openWorkspaceKeys()).toEqual(["a"]);
  });

  it("shutdown destroys every engine and later queries are refused", async () => {
    const { manager, engines } = setup({ busy: () => true });
    await manager.available();
    const port = manager.portFor({ workspaceKey: "w", root: "/w" });
    await port.findFiles(FIND);
    manager.close();
    expect(engines[0]?.destroyed).toBe(true);
    expect(await port.findFiles(FIND)).toMatchObject({ ok: false, reason: "unavailable" });
  });

  it("a library that fails to load disables the tools without retrying", async () => {
    const load = vi.fn(async (): Promise<CodeSearchEngineLoad> => ({ ok: false, error: "no dll" }));
    const manager = new CodeSearchManager({ load, isWorkspaceBusy: () => false });
    expect(await manager.available()).toBe(false);
    expect(await manager.available()).toBe(false);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("an index that fails to open is reported and retried on the next query", async () => {
    let attempts = 0;
    const open = vi.fn(async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("home directory scanning is disabled");
      return fakeEngine();
    });
    const manager = new CodeSearchManager({
      load: async () => ({ ok: true, open }),
      isWorkspaceBusy: () => false,
    });
    await manager.available();
    const port = manager.portFor({ workspaceKey: "w", root: "/w" });
    expect(await port.findFiles(FIND)).toMatchObject({
      ok: false,
      reason: "unavailable",
      message: "home directory scanning is disabled",
    });
    expect(await port.findFiles(FIND)).toMatchObject({ ok: true });
    expect(open).toHaveBeenCalledTimes(2);
  });

  it("rethrows aborts instead of reporting them as unavailable", async () => {
    const { manager } = setup();
    await manager.available();
    const controller = new AbortController();
    controller.abort(new Error("cancelled"));
    await expect(
      manager.portFor({ workspaceKey: "w", root: "/w" }).findFiles(FIND, {
        signal: controller.signal,
      }),
    ).rejects.toThrow("cancelled");
  });
});
