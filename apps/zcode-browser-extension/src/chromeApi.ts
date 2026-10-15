/**
 * The subset of the extension APIs this extension uses. Modules take it as a
 * parameter instead of reading the `chrome` global, so tests can pass fakes.
 */

export interface ChromeEvent<Listener> {
  addListener(listener: Listener): void;
  removeListener(listener: Listener): void;
}

export interface DebuggeeTarget {
  tabId?: number;
  sessionId?: string;
}

export interface ChromeTab {
  id?: number;
  windowId: number;
  url?: string;
  pendingUrl?: string;
  title?: string;
  active: boolean;
  groupId: number;
  width?: number;
  height?: number;
  lastAccessed?: number;
  incognito?: boolean;
}

export interface ChromePort {
  postMessage(message: unknown): void;
  disconnect(): void;
  onMessage: ChromeEvent<(message: unknown) => void>;
  onDisconnect: ChromeEvent<() => void>;
}

export interface ChromeStorageArea {
  get(keys: string | string[]): Promise<Record<string, unknown>>;
  set(items: Record<string, unknown>): Promise<void>;
}

export interface ChromeApi {
  runtime: {
    connectNative(application: string): ChromePort;
    getManifest(): { version: string };
    lastError?: { message?: string };
  };
  debugger: {
    attach(target: DebuggeeTarget, requiredVersion: string): Promise<void>;
    detach(target: DebuggeeTarget): Promise<void>;
    sendCommand(target: DebuggeeTarget, method: string, params?: object): Promise<unknown>;
    onEvent: ChromeEvent<(source: DebuggeeTarget, method: string, params?: unknown) => void>;
    onDetach: ChromeEvent<(source: DebuggeeTarget, reason: string) => void>;
  };
  tabs: {
    create(properties: { url?: string; active?: boolean; windowId?: number }): Promise<ChromeTab>;
    get(tabId: number): Promise<ChromeTab>;
    query(query: { windowType?: "normal" }): Promise<ChromeTab[]>;
    update(tabId: number, properties: { active?: boolean }): Promise<ChromeTab | undefined>;
    remove(tabId: number): Promise<void>;
    goBack(tabId: number): Promise<void>;
    goForward(tabId: number): Promise<void>;
    reload(tabId: number): Promise<void>;
    group(options: { tabIds: number[]; groupId?: number }): Promise<number>;
    ungroup(tabIds: number[]): Promise<void>;
    onUpdated: ChromeEvent<
      (tabId: number, change: { url?: string; title?: string }, tab: ChromeTab) => void
    >;
    onRemoved: ChromeEvent<(tabId: number) => void>;
  };
  tabGroups: {
    update(
      groupId: number,
      properties: { title?: string; color?: string; collapsed?: boolean },
    ): Promise<unknown>;
  };
  windows: {
    getLastFocused(options: { windowTypes: "normal"[] }): Promise<{ id?: number }>;
  };
  storage: { local: ChromeStorageArea };
  alarms: {
    create(name: string, info: { periodInMinutes: number }): void | Promise<void>;
    onAlarm: ChromeEvent<(alarm: { name: string }) => void>;
  };
}

/** `chrome.tabGroups.TAB_GROUP_ID_NONE`. */
export const TAB_GROUP_ID_NONE = -1;
