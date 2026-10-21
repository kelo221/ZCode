// Advisory fast type check with `bun check` (Bun 1.4.3+, TypeScript 7 semantics).
// `bun run typecheck` (tsc) stays the gate: TS7 is stricter than the repository's
// TypeScript 6 and reports a few errors tsc accepts, listed in KNOWN_TS7_ONLY.
//
//   bun apps/zcode-gpui/scripts/typecheck_fast.ts [project ...]
//
// Without arguments it checks the projects of the root `typecheck` script.
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../../..");
// Reported by TS7 only; exit status ignores them. Empty since the root
// `typecheck` stopped covering the Electron packages (2026-10-21).
const KNOWN_TS7_ONLY: string[] = [];

async function rootProjects(): Promise<string[]> {
  const pkg = JSON.parse(await readFile(join(root, "package.json"), "utf8")) as {
    scripts: Record<string, string>;
  };
  const [command, ...rest] = pkg.scripts.typecheck.trim().split(/\s+/);
  if (command !== "tsc" || rest[0] !== "-b") {
    throw new Error(`unexpected root typecheck script: ${pkg.scripts.typecheck}`);
  }
  return rest.slice(1);
}

const projects = process.argv.length > 2 ? process.argv.slice(2) : await rootProjects();
const started = performance.now();
let unexpected = 0;
for (const project of projects) {
  const t0 = performance.now();
  const proc = Bun.spawn([process.execPath, "check", "-b", "-p", project, "--no-pretty", "--all"], {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  const errors = `${out}\n${err}`
    .split(/\r?\n/)
    .filter((line) => /error TS\d+/.test(line))
    .map((line) => line.replaceAll("\\", "/"));
  const fresh = errors.filter((line) => !KNOWN_TS7_ONLY.some((known) => line.includes(known)));
  const known = errors.length - fresh.length;
  const seconds = ((performance.now() - t0) / 1000).toFixed(1);
  if (fresh.length > 0 || (code !== 0 && errors.length === 0)) {
    unexpected += Math.max(fresh.length, 1);
    console.log(`FAIL ${project} (${seconds}s)`);
    for (const line of fresh.length > 0 ? fresh : [`${out}${err}`.trim()]) console.log(`  ${line}`);
  } else {
    console.log(`ok   ${project} (${seconds}s${known ? `, ${known} known TS7-only` : ""})`);
  }
}
const total = ((performance.now() - started) / 1000).toFixed(1);
console.log(`${projects.length} projects in ${total}s; ${unexpected} unexpected error(s)`);
process.exit(unexpected > 0 ? 1 : 0);
