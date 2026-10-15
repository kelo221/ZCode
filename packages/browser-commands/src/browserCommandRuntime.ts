/**
 * 宿主相关的依赖由宿主在启动时注入：命令层本身不读文件、不写日志文件，
 * 这样同一份代码既能跑在 Electron main，也能跑在浏览器扩展 service worker 里。
 */
export interface BrowserCommandLogger {
  debug(message: string, meta?: Record<string, unknown>): void;
}

export interface BrowserCommandRuntime {
  /** Playwright injected script 源码（Apache-2.0）；Node 宿主用 `@zcode/browser-commands/node` 读取。 */
  playwrightInjectedScriptSource: () => string;
  logger?: BrowserCommandLogger;
}

let runtime: BrowserCommandRuntime | undefined;

export function configureBrowserCommandRuntime(next: BrowserCommandRuntime): void {
  runtime = next;
}

export function getPlaywrightInjectedScriptSource(): string {
  if (!runtime) {
    throw new Error(
      "Browser command runtime is not configured: call configureBrowserCommandRuntime",
    );
  }
  return runtime.playwrightInjectedScriptSource();
}

/**
 * 修复原因：命令层原先在模块顶层读 `process.platform`，浏览器扩展 service worker 没有
 * `process`，加载即抛 ReferenceError（Service worker registration failed, status 15）。
 * Node/Electron 仍优先用 `process.platform`，行为不变；浏览器里退回 navigator 平台信息。
 */
export function isMacPlatform(): boolean {
  const host = globalThis as {
    process?: { platform?: string };
    navigator?: { platform?: string; userAgentData?: { platform?: string } };
  };
  const nodePlatform = host.process?.platform;
  if (nodePlatform) return nodePlatform === "darwin";
  const nav = host.navigator;
  return /mac/i.test(nav?.userAgentData?.platform ?? nav?.platform ?? "");
}

/**
 * 修复原因：`Buffer` 只存在于 Node；扩展里读 PNG 尺寸时抛错被吞，截图尺寸归一化会被
 * 静默跳过。atob 在 Node 16+ 与浏览器中都可用。
 */
export function decodeBase64(data: string): Uint8Array {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

export const browserCommandLogger: BrowserCommandLogger = {
  debug(message, meta) {
    runtime?.logger?.debug(message, meta);
  },
};
