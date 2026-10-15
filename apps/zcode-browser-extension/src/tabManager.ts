import { isAllowedBrowserUrl } from "@zcode/browser-commands";
import type {
  BrowserDialog,
  BrowserResponseMeta,
  BrowserTabSummary,
  BrowserUserTabInfo,
  BrowserViewportSize,
} from "@zcode/shared";
import { TAB_GROUP_ID_NONE, type ChromeApi, type ChromeTab } from "./chromeApi.js";
import type { TabPageState } from "./controlledView.js";
import { TabDebugger } from "./tabDebugger.js";

/** Who controls a tab: one agent session in one workspace. */
export interface SessionScope {
  workspaceKey: string;
  remoteSessionId: string;
  sessionId: string;
}

export type TabLifecycle = "active" | "deliverable" | "handoff";

export interface ManagedTab {
  tabId: string;
  chromeTabId: number;
  scope: SessionScope;
  /** `user` tabs were claimed from the user's own tabs. */
  origin: "agent" | "user";
  lifecycle: TabLifecycle;
  viewport?: BrowserViewportSize;
  openedAt: number;
  page: TabPageState;
  debuggee: TabDebugger;
  dialog: BrowserDialog | null;
}

const DEFAULT_GROUP_TITLE = "ZCode";
const FALLBACK_VIEWPORT: BrowserViewportSize = { width: 1280, height: 720 };

export function scopeKey(scope: SessionScope): string {
  return [scope.workspaceKey, scope.remoteSessionId, scope.sessionId].join("\u0000");
}

export function tabIdFor(chromeTabId: number): string {
  return `tab-${chromeTabId}`;
}

function chromeTabIdOf(tabId: string): number | undefined {
  const match = /^tab-(\d+)$/.exec(tabId);
  return match ? Number(match[1]) : undefined;
}

/** Strip credentials, query and fragment before a URL leaves the browser in metadata. */
export function sanitizeMetaUrl(value: string): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    url.username = "";
    url.password = "";
    url.search = "";
    url.hash = "";
    const sanitized = url.toString();
    return url.pathname === "/" ? sanitized.slice(0, -1) : sanitized;
  } catch {
    return undefined;
  }
}

function isUserPage(tab: ChromeTab): boolean {
  const url = tab.url || tab.pendingUrl || "";
  return !tab.incognito && url !== "about:blank" && isAllowedBrowserUrl(url);
}

/**
 * The extension's single owner of controlled tabs. Controlled tabs sit in a
 * tab group per session; releasing a tab ungroups it and gives it back to
 * the user. Chrome tab events keep the cached page facts current.
 */
export class TabManager {
  private readonly tabs = new Map<string, ManagedTab>();
  private readonly activeByScope = new Map<string, string>();
  private readonly groupByScope = new Map<string, number>();
  private readonly nameByScope = new Map<string, string>();

  constructor(
    private readonly chrome: ChromeApi,
    private readonly now: () => number = Date.now,
  ) {}

  owned(scope: SessionScope): ManagedTab[] {
    const key = scopeKey(scope);
    return [...this.tabs.values()]
      .filter((tab) => scopeKey(tab.scope) === key)
      .sort((a, b) => a.openedAt - b.openedAt);
  }

  all(): ManagedTab[] {
    return [...this.tabs.values()];
  }

  byChromeTab(chromeTabId: number): ManagedTab | undefined {
    return this.tabs.get(tabIdFor(chromeTabId));
  }

  /** The tab a command without `tabId` lands on: the selected one, else the newest. */
  activeTab(scope: SessionScope): ManagedTab | undefined {
    const selected = this.tabs.get(this.activeByScope.get(scopeKey(scope)) ?? "");
    if (selected && scopeKey(selected.scope) === scopeKey(scope)) return selected;
    return this.owned(scope).at(-1);
  }

  resolve(scope: SessionScope, tabId?: string): ManagedTab | undefined {
    if (!tabId) return this.activeTab(scope);
    const tab = this.tabs.get(tabId);
    return tab && scopeKey(tab.scope) === scopeKey(scope) ? tab : undefined;
  }

  select(tab: ManagedTab): void {
    this.activeByScope.set(scopeKey(tab.scope), tab.tabId);
  }

  private register(chromeTab: ChromeTab, scope: SessionScope, origin: ManagedTab["origin"]) {
    const chromeTabId = chromeTab.id!;
    const tab: ManagedTab = {
      tabId: tabIdFor(chromeTabId),
      chromeTabId,
      scope: { ...scope },
      origin,
      lifecycle: "active",
      openedAt: this.now(),
      page: {
        url: chromeTab.url || chromeTab.pendingUrl || "about:blank",
        title: chromeTab.title ?? "",
        canGoBack: false,
        canGoForward: false,
      },
      debuggee: new TabDebugger(this.chrome, chromeTabId),
      dialog: null,
    };
    this.tabs.set(tab.tabId, tab);
    this.select(tab);
    return tab;
  }

  private async addToGroup(tab: ManagedTab): Promise<void> {
    const key = scopeKey(tab.scope);
    const existing = this.groupByScope.get(key);
    try {
      const groupId = await this.chrome.tabs.group({
        tabIds: [tab.chromeTabId],
        ...(existing !== undefined ? { groupId: existing } : {}),
      });
      if (groupId !== existing) {
        this.groupByScope.set(key, groupId);
        await this.chrome.tabGroups.update(groupId, {
          title: this.nameByScope.get(key) ?? DEFAULT_GROUP_TITLE,
          color: "blue",
        });
      }
    } catch {
      // The group was closed by the user; start a new one next time.
      this.groupByScope.delete(key);
      if (existing !== undefined) await this.addToGroup(tab);
    }
  }

  /** A new background tab in the user's current window, grouped for the session. */
  async create(scope: SessionScope): Promise<ManagedTab> {
    const window = await this.chrome.windows.getLastFocused({ windowTypes: ["normal"] });
    const chromeTab = await this.chrome.tabs.create({
      url: "about:blank",
      active: false,
      ...(window.id !== undefined ? { windowId: window.id } : {}),
    });
    const tab = this.register(chromeTab, scope, "agent");
    await this.addToGroup(tab);
    return tab;
  }

  /** The user's own open pages that a session may claim. */
  async userTabs(): Promise<BrowserUserTabInfo[]> {
    const open = await this.chrome.tabs.query({ windowType: "normal" });
    return open
      .filter((tab) => tab.id !== undefined && !this.tabs.has(tabIdFor(tab.id)) && isUserPage(tab))
      .sort((a, b) => Number(b.active) - Number(a.active))
      .map((tab) => ({
        id: tabIdFor(tab.id!),
        ...(tab.url ? { url: tab.url } : {}),
        ...(tab.title ? { title: tab.title } : {}),
        ...(tab.lastAccessed ? { lastOpened: new Date(tab.lastAccessed).toISOString() } : {}),
      }));
  }

  async claim(scope: SessionScope, tabId: string): Promise<ManagedTab | undefined> {
    const chromeTabId = chromeTabIdOf(tabId);
    if (chromeTabId === undefined || this.tabs.has(tabId)) return undefined;
    const chromeTab = await this.chrome.tabs.get(chromeTabId).catch(() => undefined);
    if (!chromeTab || !isUserPage(chromeTab)) return undefined;
    const tab = this.register(chromeTab, scope, "user");
    await this.addToGroup(tab);
    return tab;
  }

  /** Hand a tab back to the user: it stays open, ungrouped and undebugged. */
  async release(tab: ManagedTab): Promise<void> {
    this.forget(tab);
    await tab.debuggee.detach();
    await this.chrome.tabs.ungroup([tab.chromeTabId]).catch(() => undefined);
  }

  async close(tab: ManagedTab): Promise<void> {
    this.forget(tab);
    await tab.debuggee.detach();
    await this.chrome.tabs.remove(tab.chromeTabId).catch(() => undefined);
  }

  private forget(tab: ManagedTab): void {
    this.tabs.delete(tab.tabId);
    const key = scopeKey(tab.scope);
    if (this.activeByScope.get(key) === tab.tabId) this.activeByScope.delete(key);
    if (this.owned(tab.scope).length === 0) this.groupByScope.delete(key);
  }

  async nameSession(scope: SessionScope, name: string): Promise<void> {
    const key = scopeKey(scope);
    this.nameByScope.set(key, name);
    const groupId = this.groupByScope.get(key);
    if (groupId !== undefined && groupId !== TAB_GROUP_ID_NONE) {
      await this.chrome.tabGroups.update(groupId, { title: name }).catch(() => undefined);
    }
  }

  forgetScope(scope: SessionScope): void {
    const key = scopeKey(scope);
    this.activeByScope.delete(key);
    this.nameByScope.delete(key);
  }

  /** `chrome.tabs.onRemoved`: the user closed a controlled tab. */
  handleRemoved(chromeTabId: number): void {
    const tab = this.byChromeTab(chromeTabId);
    if (tab) this.forget(tab);
  }

  handleUpdated(chromeTabId: number, change: { url?: string; title?: string }): void {
    const tab = this.byChromeTab(chromeTabId);
    if (!tab) return;
    if (change.url !== undefined) tab.page.url = change.url;
    if (change.title !== undefined) tab.page.title = change.title;
  }

  async viewport(tab: ManagedTab): Promise<BrowserViewportSize> {
    if (tab.viewport) return { ...tab.viewport };
    const chromeTab = await this.chrome.tabs.get(tab.chromeTabId).catch(() => undefined);
    return chromeTab?.width && chromeTab.height
      ? { width: chromeTab.width, height: chromeTab.height }
      : { ...FALLBACK_VIEWPORT };
  }

  async summary(tab: ManagedTab): Promise<BrowserTabSummary> {
    return {
      tabId: tab.tabId,
      url: tab.page.url,
      title: tab.page.title,
      viewport: await this.viewport(tab),
      ...(this.activeTab(tab.scope)?.tabId === tab.tabId ? { active: true } : {}),
      ...(tab.lifecycle !== "active" ? { lifecycle: tab.lifecycle } : {}),
    };
  }

  meta(
    scope: SessionScope,
    browser: { browserId: string; generation: number },
    tab?: ManagedTab,
    lifecycle?: BrowserResponseMeta["lifecycle"],
  ): BrowserResponseMeta {
    const currentUrl = tab ? sanitizeMetaUrl(tab.page.url) : undefined;
    return {
      browserUse: true,
      backendType: "extension",
      browserId: browser.browserId,
      browserGeneration: browser.generation,
      openTabIds: this.owned(scope).map((t) => t.tabId),
      ...(tab ? { tabId: tab.tabId } : {}),
      ...(currentUrl ? { currentUrl } : {}),
      ...(lifecycle ? { lifecycle } : tab ? { lifecycle: tab.lifecycle } : {}),
    };
  }
}
