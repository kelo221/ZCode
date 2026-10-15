import { executeBrowserCommandOnView } from "@zcode/browser-commands";
import type { BrowserCommand, BrowserCommandResult } from "@zcode/shared";
import type { ChromeApi } from "./chromeApi.js";
import { createControlledView, refreshPageState } from "./controlledView.js";
import { scopeKey, type ManagedTab, type SessionScope, type TabManager } from "./tabManager.js";

/** Scope plus request identity, built from GPUI's `execute` message. */
export interface ExecutionContext extends SessionScope {
  requestId: string;
  turnId?: string;
  browserId: string;
  generation: number;
}

interface Running {
  context: ExecutionContext;
  controller: AbortController;
  dispatched: boolean;
}

type ResultBody = Omit<BrowserCommandResult, "elapsedMs" | "meta">;

const UNSUPPORTED_PLAYWRIGHT = new Set(["fileChooserSetFiles", "waitForEvent", "downloadPath"]);
const TAB_HINT = "List the session's tabs again (tabs.list()) or open a new one.";

function abortPromise(signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) resolve();
    else signal.addEventListener("abort", () => resolve(), { once: true });
  });
}

/**
 * Runs one `BrowserCommand` for one session. Tab and lifecycle commands are
 * handled here; page commands go to the shared command layer through a
 * `ControlledView` over `chrome.debugger`.
 */
export class CommandRouter {
  private readonly running = new Map<string, Running>();
  private readonly tracked = new WeakSet<ManagedTab>();

  constructor(
    private readonly chrome: ChromeApi,
    private readonly tabs: TabManager,
  ) {}

  async execute(context: ExecutionContext, command: BrowserCommand): Promise<BrowserCommandResult> {
    const startedAt = Date.now();
    const finish = (
      partial: ResultBody,
      tab?: ManagedTab,
      closed = false,
    ): BrowserCommandResult => ({
      ...partial,
      elapsedMs: Date.now() - startedAt,
      meta: this.tabs.meta(context, context, tab, closed ? "closed" : undefined),
    });
    if (command.method === "cancelRequest") {
      const target = this.running.get(command.requestId);
      const cancelled = target !== undefined && scopeKey(target.context) === scopeKey(context);
      if (cancelled) target.controller.abort();
      return finish({ ok: true, value: { cancelled } });
    }
    if (command.method === "turnEnded") {
      await this.endTurn(context, command.turnId ?? context.turnId);
      return finish({ ok: true });
    }
    if (command.method === "closeSession") {
      await this.closeSession(context, command.closeTabs === true);
      return finish({ ok: true });
    }
    if (this.running.has(context.requestId)) {
      return finish({
        ok: false,
        error: {
          code: "duplicate_request_id",
          message: `browser requestId '${context.requestId}' is already running`,
          sideEffect: "none",
        },
      });
    }
    const running: Running = { context, controller: new AbortController(), dispatched: false };
    this.running.set(context.requestId, running);
    try {
      const work = this.run(context, command, running, finish);
      const cancelled = abortPromise(running.controller.signal).then(() =>
        finish({
          ok: false,
          error: {
            code: "cancelled",
            message: "Browser command cancelled",
            sideEffect: running.dispatched ? "uncertain" : "none",
          },
        }),
      );
      return await Promise.race([work, cancelled]);
    } finally {
      if (this.running.get(context.requestId) === running) this.running.delete(context.requestId);
    }
  }

  private async run(
    context: ExecutionContext,
    command: BrowserCommand,
    running: Running,
    finish: (partial: ResultBody, tab?: ManagedTab, closed?: boolean) => BrowserCommandResult,
  ): Promise<BrowserCommandResult> {
    const unavailable = (message: string, tab?: ManagedTab) =>
      finish({ ok: false, error: { code: "backend_unavailable", message } }, tab);
    switch (command.method) {
      case "list": {
        const tabs = await Promise.all(this.tabs.owned(context).map((t) => this.tabs.summary(t)));
        return finish({ ok: true, tabs });
      }
      case "listUserTabs":
        return finish({ ok: true, userTabs: await this.tabs.userTabs() });
      case "nameSession":
        await this.tabs.nameSession(context, command.name);
        return finish({ ok: true });
      case "newTab": {
        running.dispatched = true;
        const tab = await this.tabs.create(context);
        return finish({ ok: true, tab: await this.tabs.summary(tab) }, tab);
      }
      case "claimTab": {
        const tab = await this.tabs.claim(context, command.tabId);
        if (!tab)
          return unavailable(
            `user browser tab '${command.tabId}' is unavailable or already claimed`,
          );
        return finish({ ok: true, tab: await this.tabs.summary(tab) }, tab);
      }
      case "finalizeTabs": {
        const owned = this.tabs.owned(context);
        const unknown = command.keep.filter((k) => !owned.some((t) => t.tabId === k.tabId));
        if (unknown.length > 0) {
          return unavailable(
            `cannot finalize unknown tab(s): ${unknown.map((k) => k.tabId).join(", ")}`,
          );
        }
        for (const { tabId, status } of command.keep) {
          const tab = owned.find((t) => t.tabId === tabId)!;
          tab.lifecycle = status;
          if (status === "deliverable") await this.tabs.release(tab);
        }
        return finish({ ok: true });
      }
      case "recordingStart":
      case "recordingStatus":
      case "recordingCancel":
        return finish({
          ok: false,
          error: {
            code: "capability_unsupported",
            message: "Recording is not supported by the ZCode extension",
          },
        });
      case "browserVisibilityGet": {
        const tab = this.tabs.activeTab(context);
        const chromeTab = tab
          ? await this.chrome.tabs.get(tab.chromeTabId).catch(() => undefined)
          : undefined;
        return finish({ ok: true, value: chromeTab?.active === true }, tab);
      }
    }

    const tabId = "tabId" in command ? command.tabId : undefined;
    const tab = this.tabs.resolve(context, tabId);
    if (!tab) {
      return unavailable(
        tabId
          ? `browser tab '${tabId}' is not visible in the current context. ${TAB_HINT}`
          : `no browser tab is open. ${TAB_HINT}`,
      );
    }
    switch (command.method) {
      case "activateTab":
        this.tabs.select(tab);
        return finish({ ok: true, tab: await this.tabs.summary(tab) }, tab);
      case "browserVisibilitySet":
        if (command.visible) await this.chrome.tabs.update(tab.chromeTabId, { active: true });
        return finish({ ok: true }, tab);
      case "close":
        await this.tabs.close(tab);
        return finish({ ok: true }, tab, true);
      case "finalize":
        tab.lifecycle = command.deliverable === false ? "active" : "deliverable";
        return finish({ ok: true }, tab);
      case "markDeliverable":
      case "markHandoff":
        tab.lifecycle = command.method === "markDeliverable" ? "deliverable" : "handoff";
        return finish({ ok: true }, tab);
      case "getDialog":
        return finish({ ok: true, dialog: tab.dialog }, tab);
      case "playwrightWaitForTimeout":
        await new Promise((resolve) => setTimeout(resolve, command.timeoutMs));
        return finish({ ok: true }, tab);
    }
    if (command.method === "playwright" && UNSUPPORTED_PLAYWRIGHT.has(command.action.name)) {
      return finish(
        {
          ok: false,
          error: {
            code: "capability_unsupported",
            message: "Downloads and file uploads are not supported by the ZCode extension",
          },
        },
        tab,
      );
    }

    try {
      await this.prepare(tab);
    } catch (error) {
      return unavailable(
        `could not control this tab: ${error instanceof Error ? error.message : String(error)}`,
        tab,
      );
    }
    running.dispatched = true;
    const send = (method: string, params?: unknown) => tab.debuggee.send(method, params);
    if (command.method === "handleDialog") {
      await send("Page.handleJavaScriptDialog", {
        accept: command.accept,
        ...(command.promptText !== undefined ? { promptText: command.promptText } : {}),
      });
      tab.dialog = null;
      return finish({ ok: true }, tab);
    }
    if (command.method === "browserViewportSet") {
      tab.viewport = { width: command.width, height: command.height };
      await send("Emulation.setDeviceMetricsOverride", {
        ...tab.viewport,
        deviceScaleFactor: 1,
        mobile: false,
      });
      this.tabs.select(tab);
      return finish({ ok: true }, tab);
    }
    if (command.method === "browserViewportReset") {
      tab.viewport = undefined;
      await send("Emulation.clearDeviceMetricsOverride");
      return finish({ ok: true }, tab);
    }
    const result = await executeBrowserCommandOnView(
      createControlledView(this.chrome, tab.debuggee, tab.page),
      command,
      { signal: running.controller.signal },
    );
    await refreshPageState(this.chrome, tab.debuggee, tab.page);
    const { elapsedMs: _elapsed, meta: _meta, ...partial } = result;
    return finish(partial, tab);
  }

  /** Attach the debugger and start tracking dialogs on first use. */
  private async prepare(tab: ManagedTab): Promise<void> {
    await tab.debuggee.ensureAttached();
    if (!this.tracked.has(tab)) {
      this.tracked.add(tab);
      tab.debuggee.onEvent((method, params, sessionId) => {
        if (sessionId) return;
        if (method === "Page.javascriptDialogOpening") {
          tab.dialog = {
            type: params.type as NonNullable<ManagedTab["dialog"]>["type"],
            message: String(params.message ?? ""),
            ...(typeof params.defaultPrompt === "string"
              ? { defaultPrompt: params.defaultPrompt }
              : {}),
          };
        } else if (method === "Page.javascriptDialogClosed") {
          tab.dialog = null;
        }
      });
    }
    await refreshPageState(this.chrome, tab.debuggee, tab.page);
  }

  private abortScope(scope: SessionScope, turnId?: string): void {
    for (const request of this.running.values()) {
      if (scopeKey(request.context) !== scopeKey(scope)) continue;
      if (turnId === undefined || request.context.turnId === turnId) request.controller.abort();
    }
  }

  /** Turn end: stop the turn's requests, return finished tabs, drop the debugger banner. */
  private async endTurn(scope: SessionScope, turnId?: string): Promise<void> {
    this.abortScope(scope, turnId);
    for (const tab of this.tabs.owned(scope)) {
      if (tab.lifecycle === "handoff") await tab.debuggee.detach();
      else if (tab.lifecycle === "deliverable" || tab.origin === "user")
        await this.tabs.release(tab);
      else await tab.debuggee.detach();
    }
  }

  private async closeSession(scope: SessionScope, closeTabs: boolean): Promise<void> {
    this.abortScope(scope);
    for (const tab of this.tabs.owned(scope)) {
      if (closeTabs) await this.tabs.close(tab);
      else await this.tabs.release(tab);
    }
    this.tabs.forgetScope(scope);
  }
}
