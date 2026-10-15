/**
 * Builds the unpacked extension into `dist/`: bundles the service worker
 * with the shared command layer, inlines the pinned Playwright injected
 * script, and copies the manifest and notices. Run with `bun scripts/build.ts`.
 */
import { readPlaywrightInjectedScriptSource } from "@zcode/browser-commands/node";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outdir = process.argv.includes("--outdir")
  ? process.argv[process.argv.indexOf("--outdir") + 1]!
  : join(root, "dist");

await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });

const result = await Bun.build({
  entrypoints: [join(root, "src", "background.ts")],
  outdir,
  target: "browser",
  format: "esm",
  minify: false,
  define: {
    __PLAYWRIGHT_INJECTED_SOURCE__: JSON.stringify(readPlaywrightInjectedScriptSource()),
    // zod and friends read this in a few dev-only branches.
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
});
if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

await copyFile(join(root, "manifest.json"), join(outdir, "manifest.json"));
// Apache-2.0 asks redistributions to carry the license and NOTICE file.
const playwrightDir = dirname(
  Bun.resolveSync(
    "playwright-core/package.json",
    join(root, "..", "..", "packages", "browser-commands"),
  ),
);
const [notice, playwrightNotice, playwrightLicense] = await Promise.all([
  readFile(join(root, "NOTICE.md"), "utf8"),
  readFile(join(playwrightDir, "NOTICE"), "utf8"),
  readFile(join(playwrightDir, "LICENSE"), "utf8"),
]);
await writeFile(
  join(outdir, "NOTICE.md"),
  `${notice}\n## playwright-core NOTICE\n\n${playwrightNotice}\n\n## playwright-core LICENSE\n\n${playwrightLicense}`,
);
console.log(`built ${outdir}`);
