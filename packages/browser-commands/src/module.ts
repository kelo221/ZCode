/**
 * browser-commands 模块清单（architecture-governance managed 模块）。
 * 与宿主无关的浏览器命令执行层：只依赖 `ControlledView` 抽象与 CDP 直通，
 * 不导入 Electron；Node 专属的 Playwright 源码读取位于 `node/` 适配层。
 */
export const browserCommandsModule = {
  id: "browser-commands",
  requires: ["shared"],
  provides: ["browser-commands"],
  publicEntrypoints: ["index.ts", "contract.ts", "node/index.ts"],
} as const;
