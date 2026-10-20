// Live check against the installed rust-analyzer. Skipped when rust-analyzer or cargo is not on
// PATH. Uses a throwaway crate in the temp directory.
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { findExecutableOnPath, LANGUAGE_SERVER_SPECS, LanguageServer } from "../src/lsp/index.js";

const installed =
  (await findExecutableOnPath("rust-analyzer")) !== undefined &&
  (await findExecutableOnPath("cargo")) !== undefined;

describe.skipIf(!installed)("rust-analyzer (live)", () => {
  let root: string;
  let server: LanguageServer;
  const spec = LANGUAGE_SERVER_SPECS.find((item) => item.id === "rust-analyzer")!;

  beforeAll(async () => {
    root = await mkdtemp(join(tmpdir(), "zcode-ra-live-"));
    await mkdir(join(root, "src"));
    await writeFile(
      join(root, "Cargo.toml"),
      '[package]\nname = "ra_live"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    );
    await writeFile(
      join(root, "src", "lib.rs"),
      "pub mod dep;\npub fn total() -> i32 { dep::value() + 1 }\n",
    );
    await writeFile(join(root, "src", "dep.rs"), "pub fn value() -> i32 { 1 }\n");
    server = new LanguageServer({ spec, root });
  });

  afterAll(async () => {
    await server?.close();
    await rm(root, { recursive: true, force: true }).catch(() => undefined);
  });

  async function quickUntilLoaded(file: string) {
    // The first collects after a cold start report the project as still loading.
    for (let attempt = 0; attempt < 30; attempt += 1) {
      const [result] = await server.collect({
        files: [file],
        mode: "quick",
        deadline: Date.now() + 3_000,
      });
      if (result!.freshness === "confirmed") return result!;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
    throw new Error("rust-analyzer never finished loading the test crate");
  }

  it("confirms a clean file, then sees a dependency changed on disk outside ZCode", async () => {
    const lib = join(root, "src", "lib.rs");
    expect(await quickUntilLoaded(lib)).toMatchObject({ errorCount: 0, check: "not_requested" });

    await writeFile(join(root, "src", "dep.rs"), 'pub fn value() -> String { "x".into() }\n');
    let errors = 0;
    for (let attempt = 0; attempt < 10 && errors === 0; attempt += 1) {
      const [result] = await server.collect({
        files: [lib],
        mode: "quick",
        deadline: Date.now() + 3_000,
      });
      errors = result!.errorCount;
      if (errors === 0) await new Promise((resolve) => setTimeout(resolve, 500));
    }
    expect(errors).toBeGreaterThan(0);
  }, 90_000);

  it("runs a complete project check and reports rustc errors", async () => {
    const lib = join(root, "src", "lib.rs");
    const [result] = await server.collect({
      files: [lib],
      mode: "full",
      deadline: Date.now() + 90_000,
    });
    expect(result).toMatchObject({ check: "complete" });
    expect(result!.diagnostics.some((item) => item.source === "rustc" && !item.stale)).toBe(true);
  }, 120_000);
});
