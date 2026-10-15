import type {
  ChromeApi,
  ChromeEvent,
  ChromePort,
  ChromeTab,
  DebuggeeTarget,
} from "../src/chromeApi.js";

export class FakeEvent<
  Listener extends (...args: never[]) => void,
> implements ChromeEvent<Listener> {
  readonly listeners = new Set<Listener>();
  addListener(listener: Listener) {
    this.listeners.add(listener);
  }
  removeListener(listener: Listener) {
    this.listeners.delete(listener);
  }
  emit(...args: Parameters<Listener>) {
    for (const listener of this.listeners) listener(...args);
  }
}

export interface SentCommand {
  target: DebuggeeTarget;
  method: string;
  params?: object;
}

type CommandHandler = (command: SentCommand) => unknown;

export class FakePort implements ChromePort {
  readonly sent: unknown[] = [];
  readonly onMessage = new FakeEvent<(message: unknown) => void>();
  readonly onDisconnect = new FakeEvent<() => void>();
  disconnected = false;
  postMessage(message: unknown) {
    this.sent.push(message);
  }
  disconnect() {
    this.disconnected = true;
  }
}

/** In-memory browser: tabs, groups and a scriptable debugger. */
export function createFakeChrome() {
  const tabs = new Map<number, ChromeTab>();
  let nextTabId = 100;
  let nextGroupId = 1;
  const groupTitles = new Map<number, string>();
  const attached = new Set<number>();
  const commands: SentCommand[] = [];
  const ports: FakePort[] = [];
  const handlers: CommandHandler[] = [];
  const storage: Record<string, unknown> = {};

  const debuggerEvents = new FakeEvent<
    (source: DebuggeeTarget, method: string, params?: unknown) => void
  >();
  const debuggerDetach = new FakeEvent<(source: DebuggeeTarget, reason: string) => void>();

  const defaultHandler: CommandHandler = ({ target, method }) => {
    if (method === "Page.navigate") {
      setTimeout(() => debuggerEvents.emit({ tabId: target.tabId }, "Page.loadEventFired", {}), 0);
      return { frameId: "main", loaderId: "loader-1" };
    }
    if (method === "Page.getNavigationHistory") return { currentIndex: 0, entries: [{}] };
    return {};
  };

  const chrome: ChromeApi = {
    runtime: {
      connectNative: () => {
        const port = new FakePort();
        ports.push(port);
        return port;
      },
      getManifest: () => ({ version: "0.1.0" }),
    },
    debugger: {
      attach: async (target) => {
        attached.add(target.tabId!);
      },
      detach: async (target) => {
        attached.delete(target.tabId!);
      },
      sendCommand: async (target, method, params) => {
        const command = { target, method, ...(params ? { params } : {}) };
        commands.push(command);
        for (const handler of handlers) {
          const value = handler(command);
          if (value !== undefined) return value instanceof Error ? Promise.reject(value) : value;
        }
        return defaultHandler(command);
      },
      onEvent: debuggerEvents,
      onDetach: debuggerDetach,
    },
    tabs: {
      create: async ({ url, active, windowId }) => {
        const tab: ChromeTab = {
          id: nextTabId++,
          windowId: windowId ?? 1,
          url: url ?? "about:blank",
          title: "",
          active: active ?? true,
          groupId: -1,
          width: 1200,
          height: 800,
        };
        tabs.set(tab.id!, tab);
        return { ...tab };
      },
      get: async (tabId) => {
        const tab = tabs.get(tabId);
        if (!tab) throw new Error(`No tab with id: ${tabId}`);
        return { ...tab };
      },
      query: async () => [...tabs.values()].map((t) => ({ ...t })),
      update: async (tabId, properties) => {
        const tab = tabs.get(tabId)!;
        Object.assign(tab, properties);
        return { ...tab };
      },
      remove: async (tabId) => {
        tabs.delete(tabId);
      },
      goBack: async () => {},
      goForward: async () => {},
      reload: async () => {},
      group: async ({ tabIds, groupId }) => {
        const id = groupId ?? nextGroupId++;
        for (const tabId of tabIds) tabs.get(tabId)!.groupId = id;
        return id;
      },
      ungroup: async (tabIds) => {
        for (const tabId of tabIds) {
          const tab = tabs.get(tabId);
          if (tab) tab.groupId = -1;
        }
      },
      onUpdated: new FakeEvent(),
      onRemoved: new FakeEvent(),
    },
    tabGroups: {
      update: async (groupId, { title }) => {
        if (title !== undefined) groupTitles.set(groupId, title);
        return {};
      },
    },
    windows: { getLastFocused: async () => ({ id: 1 }) },
    storage: {
      local: {
        get: async () => ({ ...storage }),
        set: async (items) => {
          Object.assign(storage, items);
        },
      },
    },
    alarms: { create: () => {}, onAlarm: new FakeEvent() },
  };

  return {
    chrome,
    tabs,
    groupTitles,
    attached,
    commands,
    ports,
    debuggerEvents,
    debuggerDetach,
    /** Answer matching commands first; return undefined to fall through. */
    handle(handler: CommandHandler) {
      handlers.push(handler);
    },
    addUserTab(url: string, title = "User page"): ChromeTab {
      const tab: ChromeTab = {
        id: nextTabId++,
        windowId: 1,
        url,
        title,
        active: false,
        groupId: -1,
        width: 1000,
        height: 700,
        lastAccessed: 1_700_000_000_000,
      };
      tabs.set(tab.id!, tab);
      return tab;
    },
  };
}
