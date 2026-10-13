// Bun source-mode entry for the unchanged Services stdio Host
// (packages/server/src/entry-stdio.ts).
//
// build-remote.ts injects `__ZCODE_VERSION__` and the Built-in Provider config as
// esbuild defines; running the entry from source has neither, so the entry throws
// before startup. Supplying the same values as globals before a dynamic import keeps
// the entry itself unchanged. Nothing may be written to stdout: it carries RPC frames.
//
// The provider config is materialized into the shared environment root that Desktop
// also uses, so it must be the production release: an explicit
// ZCODE_BUILTIN_PROVIDER_CONFIG_FILE (GPUI passes the installed Desktop's copy), else
// this checkout's production file — never the test release.
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
const { version } = JSON.parse(await readFile(resolve(root, "package.json"), "utf8")) as {
  version: string;
};
// Must be set before anything imports @zcode/shared, whose version module reads it once.
Object.assign(globalThis, { __ZCODE_VERSION__: version });

const configFile =
  process.env.ZCODE_BUILTIN_PROVIDER_CONFIG_FILE?.trim() ||
  resolve(root, "config/provider/zcode-builtin.json");
Object.assign(globalThis, {
  __ZCODE_BUILTIN_PROVIDER_CONFIG_JSON__: await readFile(configFile, "utf8"),
});

await import("../../../packages/server/src/entry-stdio.ts");
