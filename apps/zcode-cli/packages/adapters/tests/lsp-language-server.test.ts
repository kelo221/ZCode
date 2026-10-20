import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  LANGUAGE_SERVER_SPECS,
  LanguageServer,
  type LanguageServerSpec,
} from "../src/lsp/index.js";

const FIXTURE = join(__dirname, "fixtures", "fake-lsp-server.cjs");
const rustSpec = LANGUAGE_SERVER_SPECS.find((spec) => spec.id === "rust-analyzer")!;
const tsSpec = LANGUAGE_SERVER_SPECS.find((spec) => spec.id === "typescript-language-server")!;

let dir: string;
let logPath: string;
const servers: LanguageServer[] = [];

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "zcode-lsp-"));
  logPath = join(dir, "methods.log");
});

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => server.close()));
  await rm(dir, { recursive: true, force: true });
});

function startFake(
  spec: LanguageServerSpec,
  config: Record<string, unknown>,
  options: { checkStartGraceMs?: number } = {},
): LanguageServer {
  const server = new LanguageServer({
    spec,
    root: dir,
    checkStartGraceMs: options.checkStartGraceMs,
    spawnProcess: (_command, _args, cwd, env) =>
      spawn(process.execPath, [FIXTURE], {
        cwd,
        env: { ...env, FAKE_LSP: JSON.stringify(config), FAKE_LSP_LOG: logPath },
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
      }),
  });
  servers.push(server);
  return server;
}

async function methodCount(method: string): Promise<number> {
  const text = await readFile(logPath, "utf8").catch(() => "");
  return text.split("\n").filter((line) => line === method).length;
}

async function file(name: string, text: string): Promise<string> {
  const path = join(dir, name);
  await writeFile(path, text);
  return path;
}

const quick = (server: LanguageServer, files: string[], ms = 3_000) =>
  server.collect({ files, mode: "quick", deadline: Date.now() + ms });
const full = (server: LanguageServer, files: string[], ms = 10_000, signal?: AbortSignal) =>
  server.collect({ files, mode: "full", deadline: Date.now() + ms, signal });

const RUST = { pull: true, nativeSource: "rust-analyzer", quiescentAfterMs: 0 };

describe("LanguageServer with pull diagnostics", () => {
  it("returns confirmed native diagnostics, and a confirmed clean result only when the server answered", async () => {
    const server = startFake(rustSpec, RUST);
    const bad = await file("bad.rs", "fn a() {}\nlet x = ERROR;\nWARN\n");
    const good = await file("good.rs", "fn b() {}\n");
    const [badResult, goodResult] = await quick(server, [bad, good]);
    expect(badResult).toMatchObject({
      availability: "ok",
      freshness: "confirmed",
      check: "not_requested",
      timedOut: false,
      errorCount: 1,
      warningCount: 1,
    });
    expect(badResult!.diagnostics[0]).toMatchObject({ severity: "error", line: 2, column: 9 });
    expect(goodResult).toMatchObject({ freshness: "confirmed", errorCount: 0, timedOut: false });
  });

  it("reflects the file on disk when it changed outside ZCode between collects", async () => {
    const server = startFake(rustSpec, RUST);
    const path = await file("lib.rs", "fn ok() {}\n");
    expect((await quick(server, [path]))[0]!.errorCount).toBe(0);
    await writeFile(path, "fn ok() { ERROR }\n");
    expect((await quick(server, [path]))[0]!.errorCount).toBe(1);
  });

  it("never starts a project check in quick mode, however many edits there are", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { durationMs: 10 } });
    const path = await file("lib.rs", "fn a() {}\n");
    for (let edit = 0; edit < 5; edit += 1) {
      await writeFile(path, `fn a() { ${edit} }\n`);
      await quick(server, [path]);
    }
    expect(await methodCount("rust-analyzer/runFlycheck")).toBe(0);
  });
});

describe("LanguageServer project check (Rust)", () => {
  it("waits for a compiler error that arrives after a silence longer than the quiet window", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { beginDelayMs: 50, durationMs: 900 } });
    const path = await file("lib.rs", "fn a() { CHECKERR }\n");
    const [result] = await full(server, [path]);
    expect(result).toMatchObject({ check: "complete", errorCount: 1 });
    expect(result!.diagnostics[0]).toMatchObject({ source: "rustc", code: "E0308" });
    expect(result!.diagnostics[0]!.stale).toBeUndefined();
  });

  it("marks the last check's compiler errors stale in a later quick result", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { durationMs: 20 } });
    const path = await file("lib.rs", "fn a() { CHECKERR }\n");
    await full(server, [path]);
    const [result] = await quick(server, [path]);
    expect(result!.diagnostics).toHaveLength(1);
    expect(result!.diagnostics[0]).toMatchObject({ source: "rustc", stale: true });
    expect(result!.lastFullCheckAt).toBeDefined();
  });

  it("never certifies a check during which the files were edited", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { durationMs: 600 } });
    const path = await file("lib.rs", "fn a() {}\n");
    const other = await file("other.rs", "fn b() {}\n");
    const pending = full(server, [path]);
    await new Promise((resolve) => setTimeout(resolve, 150));
    await writeFile(other, "fn b() { 1 }\n");
    await quick(server, [other]);
    await writeFile(other, "fn b() { 2 }\n");
    await quick(server, [other]);
    const [result] = await pending;
    expect(result).toMatchObject({ check: "incomplete" });
    expect(result!.note).toMatch(/changed/u);
  });

  it("reports incomplete when the check outlives the deadline", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { durationMs: 5_000 } });
    const path = await file("lib.rs", "fn a() {}\n");
    const [result] = await full(server, [path], 800);
    expect(result).toMatchObject({ check: "incomplete" });
    expect(result!.note).toMatch(/deadline/u);
  });

  it("reports incomplete when the server never starts a check", async () => {
    const server = startFake(
      rustSpec,
      { ...RUST, check: { ignore: true } },
      { checkStartGraceMs: 200 },
    );
    const path = await file("lib.rs", "fn a() {}\n");
    const [result] = await full(server, [path], 3_000);
    expect(result).toMatchObject({ check: "incomplete" });
    expect(result!.note).toMatch(/did not start/u);
  });

  it("shares one check between waiters; a cancelled waiter only detaches", async () => {
    const server = startFake(rustSpec, { ...RUST, check: { durationMs: 500 } });
    const path = await file("lib.rs", "fn a() { CHECKERR }\n");
    const controller = new AbortController();
    const first = full(server, [path], 10_000, controller.signal);
    const second = full(server, [path]);
    await new Promise((resolve) => setTimeout(resolve, 100));
    controller.abort(new Error("cancelled"));
    await expect(first).rejects.toThrow("cancelled");
    const [result] = await second;
    expect(result).toMatchObject({ check: "complete", errorCount: 1 });
    expect(await methodCount("rust-analyzer/runFlycheck")).toBe(1);
  });

  it("reports best_effort while the server is still loading the project", async () => {
    const server = startFake(rustSpec, { ...RUST, quiescentAfterMs: 60_000 });
    const path = await file("lib.rs", "fn a() {}\n");
    const [result] = await quick(server, [path], 1_000);
    expect(result).toMatchObject({ freshness: "best_effort" });
    expect(result!.note).toMatch(/loading/u);
  });
});

describe("LanguageServer with unversioned push (TypeScript)", () => {
  it("merges staggered publications inside the quiet window and stays best_effort", async () => {
    const server = startFake(tsSpec, { nativeSource: "typescript", pushDelaysMs: [0, 150] });
    const path = await file("a.ts", "const a = ERROR;\n");
    const [result] = await quick(server, [path]);
    expect(result).toMatchObject({
      freshness: "best_effort",
      errorCount: 1,
      check: "not_requested",
    });
  });

  it("does not present a premature empty publication as confirmed", async () => {
    const server = startFake(tsSpec, { nativeSource: "typescript", pushDelaysMs: [0, 2_500] });
    const path = await file("a.ts", "const a = ERROR;\n");
    const [result] = await quick(server, [path], 1_500);
    expect(result).toMatchObject({ freshness: "best_effort", errorCount: 0 });
  });

  it("treats silence as a timeout, not a clean file", async () => {
    const server = startFake(tsSpec, { nativeSource: "typescript", pushDelaysMs: [60_000] });
    const path = await file("a.ts", "const a = 1;\n");
    const [result] = await quick(server, [path], 800);
    expect(result).toMatchObject({ timedOut: true, freshness: "best_effort" });
  });

  it("reports not_supported for a full check", async () => {
    const server = startFake(tsSpec, { nativeSource: "typescript" });
    const path = await file("a.ts", "const a = ERROR;\n");
    const [result] = await full(server, [path], 3_000);
    expect(result).toMatchObject({ check: "not_supported", errorCount: 1 });
  });
});

describe("LanguageServer with versioned push", () => {
  it("confirms a publication whose version matches the synced document", async () => {
    const server = startFake(tsSpec, { nativeSource: "typescript", versioned: true });
    const path = await file("a.ts", "const a = ERROR;\n");
    const [result] = await quick(server, [path]);
    expect(result).toMatchObject({ freshness: "confirmed", errorCount: 1 });
  });
});

describe("LanguageServer failures", () => {
  it("reports unavailable when the server process cannot start", async () => {
    const server = new LanguageServer({
      spec: rustSpec,
      root: dir,
      spawnProcess: (_command, _args, cwd, env) =>
        spawn(process.execPath, ["-e", "process.exit(3)"], { cwd, env, stdio: "pipe" }),
    });
    servers.push(server);
    const path = await file("lib.rs", "fn a() {}\n");
    const [result] = await quick(server, [path]);
    expect(result).toMatchObject({ availability: "unavailable" });
    expect(server.alive).toBe(false);
  });
});
