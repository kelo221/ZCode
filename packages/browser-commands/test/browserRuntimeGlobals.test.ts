import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { decodeBase64, isMacPlatform } from "../src/browserCommandRuntime.js";

const SRC = join(import.meta.dirname, "..", "src");

async function sourceFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return entry.name === "node" ? [] : sourceFiles(path);
      return entry.name.endsWith(".ts") && !entry.name.endsWith(".test.ts") ? [path] : [];
    }),
  );
  return nested.flat();
}

describe("browser-commands 运行环境", () => {
  // 扩展 service worker 没有 Node 全局：模块顶层读 process 会让 worker 注册失败（status 15）。
  it("非 node/ 源码不使用 Node 专属全局", async () => {
    const offenders: string[] = [];
    for (const file of await sourceFiles(SRC)) {
      const code = (await readFile(file, "utf8"))
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/(^|[^:])\/\/.*$/gm, "$1");
      for (const pattern of [/\bprocess\./, /\bBuffer\b/, /\brequire\(/, /from "node:/]) {
        if (pattern.test(code)) offenders.push(`${relative(SRC, file)}: ${pattern.source}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("decodeBase64 与 Node Buffer 解码一致", () => {
    const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB";
    expect(Array.from(decodeBase64(png))).toEqual(Array.from(Buffer.from(png, "base64")));
  });

  it("Node 宿主仍按 process.platform 判断 macOS", () => {
    expect(isMacPlatform()).toBe(process.platform === "darwin");
  });
});
