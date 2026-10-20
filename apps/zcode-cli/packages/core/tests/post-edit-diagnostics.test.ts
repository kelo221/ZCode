import { describe, expect, it, vi } from "vitest";
import {
  createRootTraceContext,
  createSessionId,
  createToolCallId,
  createTurnId,
  parseCompletedToolPartMetadata,
  SessionEventType,
  toolResultDisplayPayloadSchema,
  type DiagnosticsPort,
  type FileDiagnostic,
  type FileDiagnosticsResult,
  type SessionEvent,
  type TraceId,
} from "@zcode/contracts";
import { PermissionService, defaultPermissionConfig } from "../src/permission/service.js";
import { createToolExecutor } from "../src/tool/executor.js";
import { createToolRegistry, registerBuiltInTools } from "../src/tool/index.js";
import { formatDiagnosticsBlock } from "../src/tool/diagnostics-format.js";
import { diagnosticsToolEntry } from "../src/tool/handlers/diagnostics.js";
import {
  collectPostEditDiagnostics,
  mutatedFilePaths,
  POST_EDIT_DIAGNOSTICS_DEADLINE_MS,
} from "../src/tool/executor/post-edit-diagnostics.js";
import { completedToolPartMetadata } from "../src/runtime/methods/tool-part-metadata.js";
import type { ToolEntry, ToolExecutionContext } from "../src/tool/types.js";

function result(overrides: Partial<FileDiagnosticsResult> = {}): FileDiagnosticsResult {
  const diagnostics = overrides.diagnostics ?? [];
  return {
    filePath: "/work/src/lib.rs",
    server: "rust-analyzer",
    availability: "ok",
    freshness: "confirmed",
    check: "not_requested",
    timedOut: false,
    errorCount: diagnostics.filter((item) => item.severity === "error").length,
    warningCount: diagnostics.filter((item) => item.severity === "warning").length,
    diagnostics,
    knownOtherFileErrors: 0,
    ...overrides,
  };
}

function error(line: number, message = "mismatched types"): FileDiagnostic {
  return { severity: "error", line, column: 5, message, source: "rust-analyzer" };
}

function fakePort(
  results: FileDiagnosticsResult[],
): DiagnosticsPort & { collect: ReturnType<typeof vi.fn> } {
  return { collect: vi.fn(async () => results) };
}

describe("diagnostics text", () => {
  it("states counts from the full result even when the list is truncated", () => {
    const many = Array.from({ length: 40 }, (_, index) => error(index + 1));
    const text = formatDiagnosticsBlock(result({ diagnostics: many }), "/work");
    expect(text).toContain('file="src/lib.rs"');
    expect(text).toContain("40 errors, 0 warnings.");
    expect(text.split("\n").filter((line) => line.startsWith("error "))).toHaveLength(20);
    expect(text).toContain("(20 more lines omitted)");
  });

  it("stays under the byte cap with long messages", () => {
    const long = Array.from({ length: 20 }, (_, index) => error(index + 1, "x".repeat(1_000)));
    const text = formatDiagnosticsBlock(result({ diagnostics: long }), "/work");
    expect(Buffer.byteLength(text, "utf8")).toBeLessThanOrEqual(4 * 1024 + 200);
  });

  it("labels compiler diagnostics from an earlier check as possibly outdated", () => {
    const text = formatDiagnosticsBlock(
      result({
        diagnostics: [{ ...error(3), source: "rustc", stale: true }],
        lastFullCheckAt: "2026-10-20T10:00:00.000Z",
      }),
      "/work",
    );
    expect(text).toContain(
      "(rustc, from the last full check at 2026-10-20T10:00:00.000Z, may be outdated)",
    );
  });

  it("never calls a timeout or an unconfirmed empty result clean", () => {
    expect(
      formatDiagnosticsBlock(result({ timedOut: true, freshness: "best_effort" }), "/work"),
    ).toContain("this is not a clean result");
    expect(formatDiagnosticsBlock(result({ freshness: "best_effort" }), "/work")).toContain(
      "(unconfirmed)",
    );
    expect(formatDiagnosticsBlock(result(), "/work")).toContain("No errors or warnings.");
  });

  it("calls other-file errors known, not a project total, unless the check completed", () => {
    expect(formatDiagnosticsBlock(result({ knownOtherFileErrors: 2 }), "/work")).toContain(
      "2 errors in other files (not a project total)",
    );
    expect(
      formatDiagnosticsBlock(result({ knownOtherFileErrors: 2, check: "complete" }), "/work"),
    ).not.toContain("not a project total");
  });
});

describe("post-edit diagnostics hook", () => {
  it("finds mutated files for Edit/Write and ApplyPatch, skipping deletions", () => {
    expect(mutatedFilePaths({ filePath: "/a.rs", structuredPatch: [] })).toEqual(["/a.rs"]);
    expect(
      mutatedFilePaths({
        structuredPatch: [],
        summary: "",
        files: [
          { filePath: "/a.rs", type: "update" },
          { filePath: "/b.rs", type: "delete" },
          { filePath: "/c.rs", type: "move", movePath: "/d.rs" },
        ],
      }),
    ).toEqual(["/a.rs", "/d.rs"]);
    expect(mutatedFilePaths({ stdout: "x" })).toEqual([]);
  });

  it("runs one quick collect with a 3 second deadline and omits unsupported files", async () => {
    const port = fakePort([
      result({ diagnostics: [error(1)] }),
      result({ filePath: "/work/readme.md", availability: "unsupported" }),
    ]);
    const now = 1_000;
    const collected = await collectPostEditDiagnostics({
      port,
      output: { filePath: "/work/src/lib.rs", structuredPatch: [] },
      workingDirectory: "/work",
      signal: undefined,
      now: () => now,
    });
    expect(port.collect).toHaveBeenCalledWith(
      {
        files: ["/work/src/lib.rs"],
        mode: "quick",
        deadline: now + POST_EDIT_DIAGNOSTICS_DEADLINE_MS,
      },
      { signal: undefined },
    );
    expect(collected?.results).toHaveLength(1);
    expect(collected?.trailer).toContain("1 error, 0 warnings.");
  });

  it("swallows port failures so the edit still succeeds", async () => {
    const logger = { warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() };
    const collected = await collectPostEditDiagnostics({
      port: { collect: vi.fn(async () => Promise.reject(new Error("boom"))) },
      output: { filePath: "/work/src/lib.rs", structuredPatch: [] },
      workingDirectory: "/work",
      signal: undefined,
      logger: logger as never,
    });
    expect(collected).toBeUndefined();
    expect(logger.warn).toHaveBeenCalled();
  });
});

function writeLikeEntry(): ToolEntry {
  return {
    capability: "WriteLike test capability",
    metadata: {
      concurrentSafe: true,
      destructive: false,
      name: "WriteLike",
      needsApproval: false,
      readOnly: true,
      riskLevel: "low",
      sideEffectScope: "none",
      timeoutMs: 30_000,
    },
    handler: async () => ({
      filePath: "/work/src/lib.rs",
      structuredPatch: [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: ["-a", "+b"] },
      ],
    }),
    formatModelContent: () => "The file /work/src/lib.rs has been updated.",
    inputSchema: { type: "object", additionalProperties: true },
    outputSchema: { type: "object", additionalProperties: true },
    permission: {
      permission: "test",
      reason: "test tool",
      riskLevel: "low",
      sideEffectScope: "none",
      needsApproval: false,
      patternSources: ["none"],
      denyPriority: "beforeAsk",
    },
    resultBudget: { maxInlineBytes: 100_000, maxModelBytes: 100_000, strategy: "truncate" },
  };
}

describe("edit to display, end to end through the executor", () => {
  async function runEdit(port: DiagnosticsPort | undefined) {
    const sessionId = createSessionId("post-edit-diagnostics");
    const turnId = createTurnId("turn");
    const traceContext = createRootTraceContext({ sessionId, turnId });
    const events: SessionEvent[] = [];
    const registry = createToolRegistry();
    registry.register(writeLikeEntry());
    const executor = createToolExecutor({
      emitEvent: async (event) => {
        events.push(event);
      },
      permissionService: new PermissionService(defaultPermissionConfig),
      registry,
      sessionId,
      turnId,
      traceContext,
      workingDirectory: "/work",
      diagnosticsPort: port,
    } as never);
    const executed = await executor.execute(
      { id: createToolCallId("call-edit"), input: {}, name: "WriteLike" },
      { traceContext },
    );
    const event = events.find((item) => item.type === SessionEventType.ToolCallResult);
    return { executed, event };
  }

  it("appends the trailer and carries the same counts into live, persisted and V4 display", async () => {
    const many = Array.from({ length: 25 }, (_, index) => error(index + 1));
    const port = fakePort([
      result({ diagnostics: [...many, { ...error(30), severity: "warning" }] }),
    ]);
    const { executed, event } = await runEdit(port);

    expect(executed.success).toBe(true);
    const text = String(executed.modelContent);
    expect(text.startsWith("The file /work/src/lib.rs has been updated.")).toBe(true);
    expect(text).toContain("<diagnostics");
    expect(text).toContain("25 errors, 1 warning.");

    const expected = {
      errors: 25,
      warnings: 1,
      availability: "ok",
      freshness: "confirmed",
      check: "not_requested",
    };
    expect(executed.display).toMatchObject({ kind: "file_diff", diagnostics: expected });
    expect((event?.payload as any).result.display.diagnostics).toEqual(expected);

    const persisted = parseCompletedToolPartMetadata(completedToolPartMetadata(executed));
    expect(persisted?.display).toMatchObject({ kind: "file_diff", diagnostics: expected });
    expect(toolResultDisplayPayloadSchema.parse(executed.display)).toMatchObject({
      diagnostics: expected,
    });
  });

  it("leaves the edit result untouched without a port", async () => {
    const { executed } = await runEdit(undefined);
    expect(executed.modelContent).toBe("The file /work/src/lib.rs has been updated.");
    expect((executed.display as any).diagnostics).toBeUndefined();
  });

  it("still succeeds when collection throws", async () => {
    const { executed } = await runEdit({
      collect: vi.fn(async () => Promise.reject(new Error("x"))),
    });
    expect(executed.success).toBe(true);
    expect(executed.modelContent).toBe("The file /work/src/lib.rs has been updated.");
  });
});

describe("Diagnostics tool", () => {
  function context(port?: DiagnosticsPort): ToolExecutionContext {
    return {
      toolCallId: "tool-diagnostics",
      traceId: "trace-diagnostics" as TraceId,
      abortSignal: new AbortController().signal,
      diagnosticsPort: port,
      workingDirectory: "/work",
      workspaceRoot: "/work",
      sessionId: "sess-diagnostics" as never,
    };
  }

  it("is registered only with includeDiagnostics, deferred and read-only", () => {
    const without = createToolRegistry();
    registerBuiltInTools(without, {});
    expect(without.has("Diagnostics")).toBe(false);
    const withFlag = createToolRegistry();
    registerBuiltInTools(withFlag, { includeDiagnostics: true });
    expect(withFlag.has("Diagnostics")).toBe(true);
    expect(diagnosticsToolEntry.metadata).toMatchObject({ readOnly: true, shouldDefer: true });
    expect(diagnosticsToolEntry.metadata.timeoutMs).toBeGreaterThan(120_000);
  });

  it("resolves paths, runs a full check under one deadline and formats every file", async () => {
    const port = fakePort([
      result({ check: "complete", diagnostics: [error(2)] }),
      result({
        filePath: "/work/b.ts",
        server: "typescript-language-server",
        check: "not_supported",
      }),
    ]);
    const started = Date.now();
    const output = await diagnosticsToolEntry.handler(
      { paths: ["src/lib.rs", "/work/b.ts"], wait_seconds: 10 },
      context(port),
    );
    const call = port.collect.mock.calls[0]![0];
    expect(call.mode).toBe("full");
    expect(
      call.files.map((path: string) => path.replace(/\\/gu, "/").replace(/^[A-Z]:/u, "")),
    ).toEqual(["/work/src/lib.rs", "/work/b.ts"]);
    expect(call.deadline - started).toBeGreaterThanOrEqual(10_000);
    expect(call.deadline - started).toBeLessThan(11_000);
    const text = String(diagnosticsToolEntry.formatModelContent?.(output));
    expect(text).toContain('check="complete"');
    expect(text).toContain('check="not_supported"');
  });

  it("fails clearly when the session has no diagnostics port", async () => {
    const output = await diagnosticsToolEntry.handler({ paths: ["a.rs"] }, context());
    expect(output).toMatchObject({ result: false });
  });
});
