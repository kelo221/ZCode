import type { RuntimeInfo } from "@zcode/shared-types";

export { type RuntimeInfo };

// Bun/Deno 没有 node:sea 内置模块，静态 import 会让整个 bundle 在启动时崩溃；
// 与 bundled-plugins.ts 等处一致，经 process.getBuiltinModule 探测，缺失即视为非 SEA。
function isSeaRuntime(): boolean {
  const getBuiltinModule = process.getBuiltinModule as
    | ((id: "node:sea") => { isSea(): boolean } | undefined)
    | undefined;
  try {
    return getBuiltinModule?.("node:sea")?.isSea() === true;
  } catch {
    return false;
  }
}

export const getRuntimeInfo = (): RuntimeInfo => ({
  arch: process.arch,
  cwd: process.cwd(),
  execPath: process.execPath,
  node: process.version,
  platform: process.platform,
  sea: isSeaRuntime(),
  versions: process.versions,
});
