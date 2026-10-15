import type { ControlledView } from "@zcode/browser-commands";
import type { ChromeApi } from "./chromeApi.js";
import type { TabDebugger } from "./tabDebugger.js";

/**
 * Synchronous page facts the command layer reads (`getURL`, `canGoBack`...).
 * Refreshed before and after every command; `chrome.tabs.onUpdated` keeps
 * url and title current in between.
 */
export interface TabPageState {
  url: string;
  title: string;
  canGoBack: boolean;
  canGoForward: boolean;
}

const LOAD_WAIT_LIMIT_MS = 60_000;

interface RuntimeEvaluateResult {
  result?: { value?: unknown };
  exceptionDetails?: {
    text?: string;
    exception?: { description?: string; value?: unknown };
  };
}

function evaluationError(details: NonNullable<RuntimeEvaluateResult["exceptionDetails"]>): Error {
  return new Error(
    details.exception?.description ??
      (details.exception?.value == null ? undefined : String(details.exception.value)) ??
      details.text ??
      "Script evaluation failed",
  );
}

/** Resolves on the main frame's next load event; cancel when it will not come. */
function nextLoad(debuggee: TabDebugger): { promise: Promise<void>; cancel: () => void } {
  let stop = () => {};
  const promise = new Promise<void>((resolve) => {
    const timer = setTimeout(() => finish(), LOAD_WAIT_LIMIT_MS);
    const off = debuggee.onEvent((method, _params, sessionId) => {
      if (!sessionId && method === "Page.loadEventFired") finish();
    });
    const finish = () => {
      clearTimeout(timer);
      off();
      resolve();
    };
    stop = finish;
  });
  return { promise, cancel: () => stop() };
}

export async function refreshPageState(
  chrome: ChromeApi,
  debuggee: TabDebugger,
  state: TabPageState,
): Promise<void> {
  const tab = await chrome.tabs.get(debuggee.chromeTabId).catch(() => undefined);
  if (tab) {
    state.url = tab.url || tab.pendingUrl || state.url;
    state.title = tab.title ?? state.title;
  }
  if (!debuggee.isAttached) return;
  const history = (await debuggee.send("Page.getNavigationHistory").catch(() => undefined)) as
    | { currentIndex?: number; entries?: unknown[] }
    | undefined;
  if (history && typeof history.currentIndex === "number" && Array.isArray(history.entries)) {
    state.canGoBack = history.currentIndex > 0;
    state.canGoForward = history.currentIndex < history.entries.length - 1;
  }
}

export function createControlledView(
  chrome: ChromeApi,
  debuggee: TabDebugger,
  state: TabPageState,
): ControlledView {
  const tabId = debuggee.chromeTabId;
  return {
    webContents: {
      async loadURL(url) {
        await debuggee.ensureAttached();
        const load = nextLoad(debuggee);
        let navigation: { errorText?: string; loaderId?: string };
        try {
          navigation = (await debuggee.send("Page.navigate", { url })) as typeof navigation;
        } catch (error) {
          load.cancel();
          throw error;
        }
        if (navigation.errorText) {
          load.cancel();
          throw new Error(navigation.errorText);
        }
        // Same-document navigations (fragments) have no loader and no load event.
        if (!navigation.loaderId) load.cancel();
        await load.promise;
        await refreshPageState(chrome, debuggee, state);
      },
      getURL: () => state.url,
      getTitle: () => state.title,
      canGoBack: () => state.canGoBack,
      canGoForward: () => state.canGoForward,
      goBack: () => void chrome.tabs.goBack(tabId).catch(() => undefined),
      goForward: () => void chrome.tabs.goForward(tabId).catch(() => undefined),
      reload: () => void chrome.tabs.reload(tabId).catch(() => undefined),
      async executeJavaScript(script) {
        const response = (await debuggee.send("Runtime.evaluate", {
          expression: script,
          awaitPromise: true,
          returnByValue: true,
          userGesture: true,
        })) as RuntimeEvaluateResult;
        if (response.exceptionDetails) throw evaluationError(response.exceptionDetails);
        return response.result?.value;
      },
    },
    cdp: {
      send: (method, params, sessionId) => debuggee.send(method, params, sessionId),
    },
  };
}
