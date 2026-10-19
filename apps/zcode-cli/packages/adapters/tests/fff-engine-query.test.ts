import { describe, expect, it } from "vitest";
import { hasRegexSyntax, loadFffCodeSearchEngine, searchText } from "../src/code-search/fff-engine.js";

describe("fff engine query helpers", () => {
  it("strips fff constraint tokens before choosing the grep mode", () => {
    expect(searchText("useState *.tsx src/ !test")).toBe("useState");
    expect(searchText("foo apps/**/*.ts *.{ts,tsx}")).toBe("foo");
    expect(searchText("a != b")).toBe("a != b");
    expect(hasRegexSyntax(searchText("useState *.tsx"))).toBe(false);
    expect(hasRegexSyntax("fn\s+render")).toBe(true);
    expect(hasRegexSyntax("log.*Error")).toBe(true);
    expect(hasRegexSyntax("plain words")).toBe(false);
  });

  it("refuses to load outside the Bun runtime", async () => {
    // vitest runs on Node: this is the Electron/SEA situation, where fff must never be imported.
    await expect(loadFffCodeSearchEngine()).resolves.toEqual({
      ok: false,
      error: "fff requires the Bun runtime",
    });
  });
});
