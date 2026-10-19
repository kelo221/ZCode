import { describe, expect, it, vi } from "vitest";
import type { CodeSearchPort, TraceId } from "@zcode/contracts";
import { createToolRegistry, registerBuiltInTools } from "../src/tool/index.js";
import { findFilesToolEntry, searchCodeToolEntry } from "../src/tool/handlers/code-search.js";
import {
  buildExploreAllowedTools,
  formatExploreAllowedToolsForAgentDescription,
} from "../src/subagent/explore-tools.js";
import type { ToolExecutionContext } from "../src/tool/types.js";

function createContext(port?: CodeSearchPort): ToolExecutionContext {
  return {
    toolCallId: "tool-code-search",
    traceId: "trace-code-search" as TraceId,
    abortSignal: new AbortController().signal,
    codeSearchPort: port,
    workingDirectory: "/workspace",
    workspaceRoot: "/workspace",
    sessionId: "sess-code-search" as never,
  };
}

function port(overrides: Partial<CodeSearchPort> = {}): CodeSearchPort {
  return {
    findFiles: vi.fn(async () => ({ ok: true as const, files: [], totalMatched: 0 })),
    searchCode: vi.fn(async () => ({
      ok: true as const,
      matches: [],
      mode: "plain" as const,
      fuzzyFallback: false,
    })),
    ...overrides,
  };
}

const text = (entry: typeof findFilesToolEntry, output: unknown) =>
  entry.formatModelContent?.(output);

describe("FindFiles / SearchCode tools", () => {
  it("are registered only with includeCodeSearch", () => {
    const without = createToolRegistry();
    registerBuiltInTools(without, {});
    expect(without.has("FindFiles")).toBe(false);
    expect(without.has("SearchCode")).toBe(false);

    const withFlag = createToolRegistry();
    registerBuiltInTools(withFlag, { includeCodeSearch: true });
    expect(withFlag.has("FindFiles")).toBe(true);
    expect(withFlag.has("SearchCode")).toBe(true);
  });

  it("stay out of the generic Agent tool description", () => {
    for (const embeddedSearchEnabled of [true, false]) {
      const description = formatExploreAllowedToolsForAgentDescription({ embeddedSearchEnabled });
      expect(description).not.toContain("FindFiles");
      expect(description).not.toContain("SearchCode");
    }
  });

  it("are read-only, need no approval and expose flat JSON schemas", () => {
    for (const entry of [findFilesToolEntry, searchCodeToolEntry]) {
      expect(entry.metadata.readOnly).toBe(true);
      expect(entry.metadata.needsApproval).toBe(false);
      expect(entry.permission.needsApproval).toBe(false);
      const schema = JSON.stringify(entry.inputSchema);
      for (const key of ["anyOf", "$ref", "$defs", "definitions"]) {
        expect(schema).not.toContain(`"${key}"`);
      }
    }
  });

  it("FindFiles forwards the query and formats paths with git status and cursor", async () => {
    const findFiles = vi.fn(async () => ({
      ok: true as const,
      files: [{ path: "src/a.ts", gitStatus: "modified" }, { path: "src/b.ts" }],
      totalMatched: 9,
      nextCursor: "1.1",
    }));
    const output = await findFilesToolEntry.handler(
      { query: "  a.ts  ", maxResults: 2 },
      createContext(port({ findFiles })),
    );
    expect(findFiles).toHaveBeenCalledWith(
      { query: "a.ts", maxResults: 2 },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(text(findFilesToolEntry, output)).toBe(
      "2/9 matching files\nsrc/a.ts [modified]\nsrc/b.ts\ncursor: 1.1",
    );
  });

  it("SearchCode renders content with context lines grouped by file", async () => {
    const searchCode = vi.fn(async () => ({
      ok: true as const,
      mode: "regex" as const,
      fuzzyFallback: false,
      matches: [
        { path: "src/a.ts", line: 10, text: "fn render() {", before: ["// doc"], after: ["  x"] },
        { path: "src/a.ts", line: 20, text: "fn render2() {" },
        { path: "src/b.ts", line: 3, text: "render()" },
      ],
    }));
    const output = await searchCodeToolEntry.handler(
      { query: "fn\\s+render", context: 1 },
      createContext(port({ searchCode })),
    );
    expect(searchCode).toHaveBeenCalledWith(
      { query: "fn\\s+render", outputMode: "content", context: 1, maxResults: 20 },
      expect.anything(),
    );
    expect(text(searchCodeToolEntry, output)).toBe(
      [
        "3 matches (regex)",
        "src/a.ts",
        " 9-// doc",
        " 10: fn render() {",
        " 11-  x",
        " 20: fn render2() {",
        "src/b.ts",
        " 3: render()",
      ].join("\n"),
    );
  });

  it("SearchCode supports patterns, files_with_matches, count and fuzzy fallback notes", async () => {
    const matches = [
      { path: "a.rs", line: 1, text: "x" },
      { path: "a.rs", line: 2, text: "y" },
      { path: "b.rs", line: 5, text: "z" },
    ];
    const searchCode = vi.fn(async () => ({
      ok: true as const,
      mode: "plain" as const,
      fuzzyFallback: true,
      matches,
    }));
    const ctx = createContext(port({ searchCode }));
    const files = await searchCodeToolEntry.handler(
      { patterns: ["x", "y"], constraints: "*.rs", output_mode: "files_with_matches" },
      ctx,
    );
    expect(searchCode).toHaveBeenLastCalledWith(
      expect.objectContaining({ patterns: ["x", "y"], constraints: "*.rs" }),
      expect.anything(),
    );
    expect(text(searchCodeToolEntry, files)).toBe(
      "No exact matches; showing fuzzy matches.\n2 files\na.rs\nb.rs",
    );
    const counts = await searchCodeToolEntry.handler({ query: "x", output_mode: "count" }, ctx);
    expect(text(searchCodeToolEntry, counts)).toContain("3 matches in 2 files\na.rs: 2\nb.rs: 1");
  });

  it("SearchCode requires exactly one of query or patterns", async () => {
    const ctx = createContext(port());
    await expect(searchCodeToolEntry.handler({}, ctx)).rejects.toThrow();
    await expect(
      searchCodeToolEntry.handler({ query: "a", patterns: ["b"] }, ctx),
    ).rejects.toThrow();
  });

  it("turns unavailable and expired-cursor outcomes into tool failures", async () => {
    const unavailable = await findFilesToolEntry.handler(
      { query: "x" },
      createContext(
        port({
          findFiles: async () => ({ ok: false, reason: "unavailable", message: "boom" }),
        }),
      ),
    );
    expect(unavailable).toMatchObject({
      result: false,
      message: expect.stringContaining("Use Bash (rg/find) instead"),
    });
    const expired = await searchCodeToolEntry.handler(
      { query: "x", cursor: "1.9" },
      createContext(
        port({
          searchCode: async () => ({ ok: false, reason: "cursor_expired", message: "expired" }),
        }),
      ),
    );
    expect(expired).toMatchObject({ result: false, message: "expired" });
    const noPort = await searchCodeToolEntry.handler({ query: "x" }, createContext(undefined));
    expect(noPort).toMatchObject({ result: false });
  });
});
