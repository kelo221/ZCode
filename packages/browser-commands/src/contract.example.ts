import type { BrowserCommandRuntime, ControlledView } from "./contract.js";

/** 宿主需要提供的最小视图：一个已加载页面，CDP 直通记录调用方法。 */
export function createExampleControlledView(cdpCalls: string[] = []): ControlledView {
  let url = "https://example.com/";
  return {
    webContents: {
      loadURL: async (next) => {
        url = next;
      },
      getURL: () => url,
      getTitle: () => "Example Domain",
      canGoBack: () => false,
      canGoForward: () => false,
      goBack: () => undefined,
      goForward: () => undefined,
      reload: () => undefined,
      executeJavaScript: async () => ({
        scrollX: 0,
        scrollY: 0,
        innerWidth: 1280,
        innerHeight: 720,
      }),
    },
    cdp: {
      send: async (method) => {
        cdpCalls.push(method);
        return {};
      },
    },
  };
}

/** 非 Node 宿主（扩展）在构建期内嵌 Playwright 源码后以同样的形状注入。 */
export const exampleRuntime: BrowserCommandRuntime = {
  playwrightInjectedScriptSource: () => "module.exports = {};",
  logger: { debug: () => undefined },
};
