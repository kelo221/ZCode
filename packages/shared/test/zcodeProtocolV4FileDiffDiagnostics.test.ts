// Fork: the file_diff display carries an optional diagnostics summary (GPUI language-server
// diagnostics). The V4 mirror must keep it, or the edit card loses its counts on the wire.
import { describe, expect, it } from "vitest";
import { zcodeSessionRuntimePreferencesResultSchema } from "../src/zcode-protocol/index.js";
import { toolOutputSchema } from "../src/zcode-protocol-v4/index.js";

const display = {
  kind: "file_diff",
  filePath: "/work/src/lib.rs",
  additions: 1,
  deletions: 1,
  structuredPatch: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: ["-a", "+b"] }],
  truncated: false,
};

describe("file_diff diagnostics summary", () => {
  it("survives the V4 tool output schema", () => {
    const diagnostics = {
      errors: 2,
      warnings: 1,
      availability: "ok",
      freshness: "best_effort",
      check: "not_requested",
      timedOut: true,
    };
    const parsed = toolOutputSchema.parse({ text: "ok", display: { ...display, diagnostics } });
    expect(parsed.display).toEqual({ ...display, diagnostics });
  });

  it("keeps old displays without a summary", () => {
    expect(toolOutputSchema.parse({ text: "ok", display }).display).toEqual(display);
  });

  it("drops only the display, not the output, for an unknown summary shape", () => {
    const parsed = toolOutputSchema.parse({
      text: "ok",
      display: {
        ...display,
        diagnostics: {
          errors: 1,
          warnings: 0,
          availability: "ok",
          freshness: "confirmed",
          check: "complete",
          extra: 1,
        },
      },
    });
    expect(parsed.text).toBe("ok");
    expect(parsed.display).toBeUndefined();
  });

  it("accepts lspDiagnosticsEnabled in runtime preferences and leaves it optional", () => {
    const base = { nativeSearchEnhancementsEnabled: true };
    expect(
      zcodeSessionRuntimePreferencesResultSchema.parse({ ...base, lspDiagnosticsEnabled: true })
        .lspDiagnosticsEnabled,
    ).toBe(true);
    expect(
      zcodeSessionRuntimePreferencesResultSchema.parse(base).lspDiagnosticsEnabled,
    ).toBeUndefined();
  });
});
