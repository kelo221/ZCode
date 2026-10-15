import { describe, expect, it } from "vitest";
import {
  PLAYWRIGHT_DEFAULT_TIMEOUT_MS,
  executeBrowserCommandOnView,
  isAllowedBrowserUrl,
  normalizePlaywrightTimeout,
} from "./contract.js";
import { createExampleControlledView } from "./contract.example.js";

describe("browser-commands contract", () => {
  it("getState 只通过 ControlledView 读取页面状态", async () => {
    const result = await executeBrowserCommandOnView(createExampleControlledView(), {
      method: "getState",
    });
    expect(result.ok).toBe(true);
    expect(result.state).toMatchObject({
      url: "https://example.com/",
      title: "Example Domain",
      viewportWidth: 1280,
      viewportHeight: 720,
    });
    expect(typeof result.elapsedMs).toBe("number");
  });

  it("导航白名单只放行 http、https 与精确的 about:blank", async () => {
    expect(isAllowedBrowserUrl("https://example.com")).toBe(true);
    expect(isAllowedBrowserUrl("http://127.0.0.1:3000")).toBe(true);
    expect(isAllowedBrowserUrl("about:blank")).toBe(true);
    expect(isAllowedBrowserUrl("about:settings")).toBe(false);
    expect(isAllowedBrowserUrl("chrome://settings")).toBe(false);
    expect(isAllowedBrowserUrl("file:///C:/Windows")).toBe(false);

    const cdpCalls: string[] = [];
    const view = createExampleControlledView(cdpCalls);
    const blocked = await executeBrowserCommandOnView(view, {
      method: "navigate",
      url: "chrome://settings",
    });
    expect(blocked.error?.code).toBe("navigation_blocked");
    expect(view.webContents.getURL()).toBe("https://example.com/");
    expect(cdpCalls).toEqual([]);
  });

  it("Playwright 超时默认 3 秒且不超过调用点上限", () => {
    expect(normalizePlaywrightTimeout(undefined)).toBe(PLAYWRIGHT_DEFAULT_TIMEOUT_MS);
    expect(normalizePlaywrightTimeout(60_000, 10_000)).toBe(10_000);
    expect(normalizePlaywrightTimeout(-5)).toBe(0);
  });
});
