import { afterEach, describe, expect, it, vi } from "vitest";
import type { FileDiagnosticsResult } from "@zcode/contracts";
import type { CollectRequest, LanguageServerSpec } from "@zcode/adapters";
import {
  DiagnosticsManager,
  type DiagnosticsServerHandle,
} from "../src/zcode-protocol/diagnostics-manager.js";

interface FakeServer extends DiagnosticsServerHandle {
  root: string;
  closed: boolean;
  starting: boolean;
  requests: CollectRequest[];
  release(): void;
  crash(): void;
}

function okResult(filePath: string, server: string): FileDiagnosticsResult {
  return {
    filePath,
    server,
    availability: "ok",
    freshness: "confirmed",
    check: "not_requested",
    timedOut: false,
    errorCount: 0,
    warningCount: 0,
    diagnostics: [],
    knownOtherFileErrors: 0,
  };
}

function setup(
  options: {
    maxServers?: number;
    holdCollects?: boolean;
    installed?: boolean;
    workspaceBusy?: boolean;
  } = {},
) {
  const servers: FakeServer[] = [];
  let now = 0;
  const createServer = vi.fn(
    (spec: LanguageServerSpec, root: string, _env?: Record<string, string>): FakeServer => {
      const exitListeners: Array<() => void> = [];
      let gates: Array<() => void> = [];
      let active = 0;
      const server: FakeServer = {
        root,
        closed: false,
        starting: false,
        requests: [],
        alive: true,
        isBusy: () => server.starting || active > 0,
        collect: async (request) => {
          server.requests.push(request);
          active += 1;
          try {
            if (options.holdCollects) {
              await new Promise<void>((resolve, reject) => {
                gates.push(resolve);
                request.signal?.addEventListener("abort", () => reject(request.signal?.reason), {
                  once: true,
                });
              });
            }
            return request.files.map((file) => okResult(file, spec.id));
          } finally {
            active -= 1;
          }
        },
        close: vi.fn(async () => {
          server.closed = true;
        }),
        onExit: (listener) => exitListeners.push(listener),
        release: () => {
          const pending = gates;
          gates = [];
          for (const resolve of pending) resolve();
        },
        crash: () => {
          (server as { alive: boolean }).alive = false;
          for (const listener of exitListeners) listener();
        },
      };
      servers.push(server);
      return server;
    },
  );
  const manager = new DiagnosticsManager({
    isWorkspaceBusy: () => options.workspaceBusy ?? false,
    maxServers: options.maxServers ?? 4,
    idleCloseMs: 1_000,
    now: () => now,
    createServer,
    isInstalled: async () => options.installed ?? true,
    // Each top-level directory is its own project root.
    findRoot: async (_spec, filePath) => filePath.split("/").slice(0, 2).join("/"),
  });
  const port = manager.portFor({ workspaceKey: "ws" });
  return {
    manager,
    port,
    servers,
    createServer,
    advance: (ms: number) => {
      now += ms;
    },
  };
}

const collect = (
  port: ReturnType<typeof setup>["port"],
  files: string[],
  ms = 2_000,
  signal?: AbortSignal,
) => port.collect({ files, mode: "quick", deadline: Date.now() + ms }, { signal });

let managers: DiagnosticsManager[] = [];
afterEach(async () => {
  await Promise.all(managers.map((manager) => manager.close()));
  managers = [];
});

function tracked<T extends ReturnType<typeof setup>>(env: T): T {
  managers.push(env.manager);
  return env;
}

describe("DiagnosticsManager", () => {
  it("starts a worktree's server with the scope's environment", async () => {
    const env = tracked(setup());
    const scoped = env.manager.portFor({
      workspaceKey: "ws",
      env: { CARGO_TARGET_DIR: "/a/target" },
    });
    await collect(scoped, ["/wt/lib.rs"]);
    await collect(env.port, ["/a/lib.rs"]);
    expect(env.createServer.mock.calls.map(([, root, serverEnv]) => [root, serverEnv])).toEqual([
      ["/wt", { CARGO_TARGET_DIR: "/a/target" }],
      ["/a", undefined],
    ]);
  });

  it("reports unsupported file types and missing servers without starting anything", async () => {
    const env = tracked(setup({ installed: false }));
    const results = await collect(env.port, ["/a/readme.md", "/a/lib.rs"]);
    expect(results.map((result) => result.availability)).toEqual(["unsupported", "unavailable"]);
    expect(results[1]!.note).toMatch(/not installed/u);
    expect(env.createServer).not.toHaveBeenCalled();
  });

  it("groups files by server and root, shares one deadline and keeps request order", async () => {
    const env = tracked(setup());
    const results = await collect(env.port, ["/a/x.rs", "/b/y.ts", "/a/z.rs", "/a/x.rs"]);
    expect(results.map((result) => result.filePath)).toEqual(["/a/x.rs", "/b/y.ts", "/a/z.rs"]);
    expect(env.servers.map((server) => server.root).sort()).toEqual(["/a", "/b"]);
    const deadlines = env.servers.flatMap((server) =>
      server.requests.map((request) => request.deadline),
    );
    expect(new Set(deadlines).size).toBe(1);
    expect(env.servers.find((server) => server.root === "/a")!.requests[0]!.files).toEqual([
      "/a/x.rs",
      "/a/z.rs",
    ]);
  });

  it("returns capacity within the deadline when every server is busy, and keeps them alive", async () => {
    const env = tracked(setup({ maxServers: 2, holdCollects: true }));
    const first = collect(env.port, ["/a/x.rs"], 10_000);
    const second = collect(env.port, ["/b/x.rs"], 10_000);
    await vi.waitFor(() => expect(env.servers).toHaveLength(2));
    const started = Date.now();
    const [third] = await collect(env.port, ["/c/x.rs"], 400);
    expect(third).toMatchObject({ availability: "capacity", timedOut: true });
    expect(Date.now() - started).toBeLessThan(2_000);
    expect(env.servers.every((server) => !server.closed)).toBe(true);
    for (const server of env.servers) server.release();
    await Promise.all([first, second]);
  });

  it("counts a starting server toward the cap and never evicts it", async () => {
    const env = tracked(setup({ maxServers: 1 }));
    await collect(env.port, ["/a/x.rs"]);
    env.servers[0]!.starting = true;
    const [result] = await collect(env.port, ["/b/x.rs"], 300);
    expect(result!.availability).toBe("capacity");
    expect(env.servers[0]!.closed).toBe(false);
  });

  it("serves a queued request once a busy server becomes idle, evicting the least recently used", async () => {
    const env = tracked(setup({ maxServers: 1, holdCollects: true }));
    const first = collect(env.port, ["/a/x.rs"], 10_000);
    await vi.waitFor(() => expect(env.servers).toHaveLength(1));
    const queued = collect(env.port, ["/b/x.rs"], 5_000);
    setTimeout(() => env.servers[0]!.release(), 200);
    await first;
    await vi.waitFor(() => expect(env.servers).toHaveLength(2));
    env.servers[1]!.release();
    const [result] = await queued;
    expect(result!.availability).toBe("ok");
    expect(env.servers[0]!.closed).toBe(true);
  });

  it("a cancelled waiter only detaches; the server becomes idle again", async () => {
    const env = tracked(setup({ maxServers: 1, holdCollects: true }));
    const controller = new AbortController();
    const cancelled = collect(env.port, ["/a/x.rs"], 10_000, controller.signal);
    await vi.waitFor(() => expect(env.servers).toHaveLength(1));
    controller.abort(new Error("stop"));
    await expect(cancelled).rejects.toThrow("stop");
    const next = collect(env.port, ["/b/x.rs"], 5_000);
    await vi.waitFor(() => expect(env.servers).toHaveLength(2));
    env.servers[1]!.release();
    expect((await next)[0]!.availability).toBe("ok");
  });

  it("closes servers after the idle period only while their workspace has no active agent", async () => {
    const busyEnv = tracked(setup({ workspaceBusy: true }));
    await collect(busyEnv.port, ["/a/x.rs"]);
    busyEnv.manager.tick();
    busyEnv.advance(5_000);
    busyEnv.manager.tick();
    expect(busyEnv.servers[0]!.closed).toBe(false);

    const env = tracked(setup());
    await collect(env.port, ["/a/x.rs"]);
    env.manager.tick();
    env.advance(500);
    env.manager.tick();
    expect(env.servers[0]!.closed).toBe(false);
    env.advance(600);
    env.manager.tick();
    expect(env.servers[0]!.closed).toBe(true);
    expect(env.manager.runningServers()).toEqual([]);
  });

  it("forgets a crashed server and starts a new one on the next request", async () => {
    const env = tracked(setup());
    await collect(env.port, ["/a/x.rs"]);
    env.servers[0]!.crash();
    expect(env.manager.runningServers()).toEqual([]);
    expect((await collect(env.port, ["/a/x.rs"]))[0]!.availability).toBe("ok");
    expect(env.servers).toHaveLength(2);
  });

  it("closes every server on shutdown and refuses new work", async () => {
    const env = tracked(setup());
    await collect(env.port, ["/a/x.rs", "/b/y.ts"]);
    await env.manager.close();
    expect(env.servers.every((server) => server.closed)).toBe(true);
    expect((await collect(env.port, ["/a/x.rs"]))[0]!.availability).toBe("unavailable");
  });
});
