# browser-commands

与宿主无关的浏览器命令执行层。输入是 `@zcode/shared` 的 `BrowserCommand`，输出是 `BrowserCommandResult`；页面访问只经过宿主提供的 `ControlledView`（导航方法 + `cdp.send` 直通）。Desktop 内置浏览器（Electron `webContents.debugger`）与 GPUI 的浏览器扩展（`chrome.debugger`）共用这一层。

类型与测试之外的不变量：

- 模块根目录（app 层）不得导入 Electron 或 `node:*`；需要随机数时使用 `globalThis.crypto`。Node 专属代码只放在 `node/`（adapters 层），由 `@zcode/browser-commands/node` 暴露。
- 宿主必须在执行 Playwright 相关命令（`playwright` 动作里的 DOM 快照与 locator）前调用 `configureBrowserCommandRuntime`，注入 Playwright injected script 源码；未注入时这些命令以 `execution_error` 结构化失败，不会退回手写 ARIA 猜测。Node 宿主用 `readPlaywrightInjectedScriptSource()`，扩展在构建期内嵌同一字符串（Playwright 为 Apache-2.0，随附 NOTICE）。
- 导航白名单由 `isAllowedBrowserUrl` 决定：只允许 http、https 和精确的 `about:blank`。所有宿主都必须在自己的导航入口复用它，而不是各写一份。
- 命令执行不 throw：失败以 `{ ok: false, error }` 返回，可能已产生页面副作用时带 `sideEffect: "uncertain"`。`AbortSignal` 取消例外，由调用方处理。
- 跨进程 iframe 通过 `Target.attachToTarget({ flatten: true })` 取得 `sessionId`，再用 `cdp.send(method, params, sessionId)` 访问。宿主若不允许该调用（例如 `chrome.debugger`），应在自己的 `cdp.send` 里用自动附加得到的会话模拟它；拒绝时命令层会降级为跳过该 frame。
- Playwright 常规操作默认 3 秒失败预算（`normalizePlaywrightTimeout`），上限由调用点决定。
