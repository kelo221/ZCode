import { configureBrowserCommandRuntime } from "@zcode/browser-commands";
import type { BrowserCommand } from "@zcode/shared";
import { beforeEach, describe, expect, it } from "vitest";
import { CommandRouter, type ExecutionContext } from "../src/commandRouter.js";
import { TabManager } from "../src/tabManager.js";
import { createFakeChrome } from "./fakeChrome.js";

configureBrowserCommandRuntime({ playwrightInjectedScriptSource: () => "module.exports = {};" });

let fake: ReturnType<typeof createFakeChrome>;
let tabs: TabManager;
let router: CommandRouter;
let nextRequest = 0;

function context(sessionId = "s1", turnId = "t1"): ExecutionContext {
  nextRequest += 1;
  return {
    requestId: `r${nextRequest}`,
    turnId,
    sessionId,
    workspaceKey: "ws",
    remoteSessionId: "",
    browserId: "extension:p",
    generation: 7,
  };
}

const run = (command: BrowserCommand, sessionId?: string) =>
  router.execute(context(sessionId), command);

beforeEach(() => {
  fake = createFakeChrome();
  tabs = new TabManager(
    fake.chrome,
    (() => {
      let t = 0;
      return () => ++t;
    })(),
  );
  router = new CommandRouter(fake.chrome, tabs);
  fake.debuggerEvents.addListener((source, method, params) => {
    tabs.byChromeTab(source.tabId!)?.debuggee.handleEvent(source, method, params);
  });
});

describe("tabs", () => {
  it("newTab opens a background tab in a ZCode group and lists it as active", async () => {
    const created = await run({ method: "newTab" });
    expect(created.ok).toBe(true);
    expect(created.tab).toMatchObject({ url: "about:blank", active: true });
    expect(created.tab?.viewport).toEqual({ width: 1200, height: 800 });
    expect(created.meta).toMatchObject({
      backendType: "extension",
      browserId: "extension:p",
      browserGeneration: 7,
      openTabIds: [created.tab!.tabId],
    });
    const chromeTab = [...fake.tabs.values()][0]!;
    expect(chromeTab.active).toBe(false);
    expect(fake.groupTitles.get(chromeTab.groupId)).toBe("ZCode");

    await run({ method: "nameSession", name: "Fix login" });
    expect(fake.groupTitles.get(chromeTab.groupId)).toBe("Fix login");
    const list = await run({ method: "list" });
    expect(list.tabs?.map((t) => t.tabId)).toEqual([created.tab!.tabId]);
  });

  it("another session cannot see or drive the tab", async () => {
    const created = await run({ method: "newTab" });
    const foreign = await run({ method: "getState", tabId: created.tab!.tabId }, "s2");
    expect(foreign.error?.code).toBe("backend_unavailable");
    expect((await run({ method: "list" }, "s2")).tabs).toEqual([]);
  });

  it("a command without any tab explains how to recover", async () => {
    const result = await run({ method: "getState" });
    expect(result.error?.code).toBe("backend_unavailable");
    expect(result.error?.message).toContain("tabs.list()");
  });

  it("close removes the tab and reports the closed lifecycle", async () => {
    const created = await run({ method: "newTab" });
    const closed = await run({ method: "close", tabId: created.tab!.tabId });
    expect(closed.meta?.lifecycle).toBe("closed");
    expect(fake.tabs.size).toBe(0);
  });
});

describe("page commands", () => {
  it("navigate attaches the debugger, waits for load and refreshes state", async () => {
    await run({ method: "newTab" });
    const chromeTab = [...fake.tabs.values()][0]!;
    fake.handle(({ method }) => {
      if (method === "Page.navigate") {
        chromeTab.url = "https://example.com/";
        chromeTab.title = "Example Domain";
      }
      return undefined;
    });
    const result = await run({ method: "navigate", url: "https://example.com/" });
    expect(result.ok).toBe(true);
    expect(result.state).toMatchObject({ url: "https://example.com/", title: "Example Domain" });
    expect(result.meta?.currentUrl).toBe("https://example.com");
    expect(fake.attached.has(chromeTab.id!)).toBe(true);
    const methods = fake.commands.map((c) => c.method);
    expect(methods.slice(0, 3)).toEqual([
      "Page.enable",
      "Emulation.setFocusEmulationEnabled",
      "Target.setAutoAttach",
    ]);
    expect(methods).toContain("Page.navigate");
  });

  it("navigation outside http(s) is refused before touching the page", async () => {
    await run({ method: "newTab" });
    const result = await run({ method: "navigate", url: "chrome://settings" });
    expect(result.error?.code).toBe("navigation_blocked");
    expect(fake.commands.map((c) => c.method)).not.toContain("Page.navigate");
  });

  it("evaluate runs through Runtime.evaluate", async () => {
    await run({ method: "newTab" });
    fake.handle(({ method, params }) =>
      method === "Runtime.evaluate" &&
      String((params as { expression: string }).expression).includes("1 + 1")
        ? { result: { value: { ok: true, kind: "json", data: "2" } } }
        : undefined,
    );
    const result = await run({ method: "evaluate", expression: "1 + 1" });
    expect(result).toMatchObject({ ok: true, value: 2 });
    expect(fake.commands.find((c) => c.method === "Runtime.evaluate")?.params).toMatchObject({
      awaitPromise: true,
      returnByValue: true,
    });
  });

  it("dialogs are tracked from debugger events and answered through CDP", async () => {
    const created = await run({ method: "newTab" });
    await run({ method: "getState" });
    const chromeTabId = [...fake.tabs.keys()][0]!;
    fake.debuggerEvents.emit({ tabId: chromeTabId }, "Page.javascriptDialogOpening", {
      type: "confirm",
      message: "Leave?",
    });
    expect((await run({ method: "getDialog" })).dialog).toEqual({
      type: "confirm",
      message: "Leave?",
    });
    const handled = await run({ method: "handleDialog", accept: true, tabId: created.tab!.tabId });
    expect(handled.ok).toBe(true);
    expect(fake.commands.at(-1)).toMatchObject({
      method: "Page.handleJavaScriptDialog",
      params: { accept: true },
    });
    expect((await run({ method: "getDialog" })).dialog).toBeNull();
  });

  it("recording and file uploads are reported as unsupported", async () => {
    await run({ method: "newTab" });
    expect((await run({ method: "recordingStart" })).error?.code).toBe("capability_unsupported");
    const upload = await run({
      method: "playwright",
      action: { name: "fileChooserSetFiles", fileChooserId: "f", files: [] },
    } as unknown as BrowserCommand);
    expect(upload.error?.code).toBe("capability_unsupported");
  });
});

describe("lifecycle", () => {
  it("cancelRequest stops a running command of the same session", async () => {
    await run({ method: "newTab" });
    const waiting = router.execute(
      { ...context(), requestId: "long" },
      { method: "playwrightWaitForTimeout", timeoutMs: 60_000 },
    );
    const otherSession = await run({ method: "cancelRequest", requestId: "long" }, "s2");
    expect(otherSession.value).toEqual({ cancelled: false });
    const cancel = await run({ method: "cancelRequest", requestId: "long" });
    expect(cancel.value).toEqual({ cancelled: true });
    const result = await waiting;
    expect(result.error).toMatchObject({ code: "cancelled", sideEffect: "none" });
  });

  it("turn end hands deliverable tabs back and detaches the debugger from the rest", async () => {
    const kept = await run({ method: "newTab" });
    const delivered = await run({ method: "newTab" });
    await run({ method: "getState", tabId: kept.tab!.tabId });
    await run({ method: "markDeliverable", tabId: delivered.tab!.tabId });
    await run({ method: "turnEnded", turnId: "t1" });
    expect(fake.attached.size).toBe(0);
    const list = await run({ method: "list" });
    expect(list.tabs?.map((t) => t.tabId)).toEqual([kept.tab!.tabId]);
    const deliveredChrome = fake.tabs.get(Number(delivered.tab!.tabId.slice(4)))!;
    expect(deliveredChrome.groupId).toBe(-1);
  });

  it("finalizeTabs rejects unknown tabs and closeSession can close everything", async () => {
    const created = await run({ method: "newTab" });
    const bad = await run({
      method: "finalizeTabs",
      keep: [{ tabId: "tab-999", status: "handoff" }],
    });
    expect(bad.error?.code).toBe("backend_unavailable");
    const ok = await run({
      method: "finalizeTabs",
      keep: [{ tabId: created.tab!.tabId, status: "handoff" }],
    });
    expect(ok.ok).toBe(true);
    expect((await run({ method: "list" })).tabs?.[0]?.lifecycle).toBe("handoff");
    await run({ method: "closeSession", closeTabs: true });
    expect(fake.tabs.size).toBe(0);
  });
});

describe("user tabs", () => {
  it("lists the user's web pages and lets a session claim one", async () => {
    const page = fake.addUserTab("https://mail.example.com/inbox", "Inbox");
    fake.addUserTab("chrome://settings", "Settings");
    await run({ method: "newTab" });
    const listed = await run({ method: "listUserTabs" });
    expect(listed.userTabs).toEqual([
      {
        id: `tab-${page.id}`,
        url: "https://mail.example.com/inbox",
        title: "Inbox",
        lastOpened: new Date(1_700_000_000_000).toISOString(),
      },
    ]);
    const claimed = await run({ method: "claimTab", tabId: `tab-${page.id}` });
    expect(claimed.tab).toMatchObject({ url: "https://mail.example.com/inbox", active: true });
    expect(page.groupId).not.toBe(-1);
    expect((await run({ method: "claimTab", tabId: `tab-${page.id}` }, "s2")).ok).toBe(false);
    expect((await run({ method: "listUserTabs" })).userTabs).toEqual([]);

    // Claimed user tabs go back to the user at the end of the turn.
    await run({ method: "turnEnded" });
    expect(page.groupId).toBe(-1);
    expect((await run({ method: "listUserTabs" })).userTabs).toHaveLength(1);
  });
});
