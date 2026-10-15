import { configureBrowserCommandRuntime } from "@zcode/browser-commands";
import { BridgeConnection, type BridgeIdentity } from "./bridgeConnection.js";
import type { ChromeApi } from "./chromeApi.js";
import { CommandRouter } from "./commandRouter.js";
import { TabManager } from "./tabManager.js";

/** Inlined at build time from the pinned playwright-core (Apache-2.0). */
declare const __PLAYWRIGHT_INJECTED_SOURCE__: string;
declare const chrome: ChromeApi;

const KEEPALIVE_ALARM = "zcode-bridge-keepalive";
const PROFILE_ID_KEY = "profileId";
const PROFILE_LABEL_KEY = "profileLabel";

configureBrowserCommandRuntime({
  playwrightInjectedScriptSource: () => __PLAYWRIGHT_INJECTED_SOURCE__,
});

/** The vendor brand ("Google Chrome", "Helium"...) rather than the engine. */
function browserName(): string {
  const brands =
    (navigator as Navigator & { userAgentData?: { brands?: { brand: string }[] } }).userAgentData
      ?.brands ?? [];
  const vendor = brands.find((b) => !/chromium|not.?a.?brand/i.test(b.brand));
  return vendor?.brand ?? "Chromium";
}

async function identity(): Promise<BridgeIdentity> {
  const stored = await chrome.storage.local.get([PROFILE_ID_KEY, PROFILE_LABEL_KEY]);
  let profileId = typeof stored[PROFILE_ID_KEY] === "string" ? stored[PROFILE_ID_KEY] : "";
  if (!profileId) {
    profileId = crypto.randomUUID();
    await chrome.storage.local.set({ [PROFILE_ID_KEY]: profileId });
  }
  const label = stored[PROFILE_LABEL_KEY];
  return {
    profileId,
    browser: browserName(),
    profile: typeof label === "string" && label.trim() ? label : "Default",
    extensionVersion: chrome.runtime.getManifest().version,
  };
}

const tabs = new TabManager(chrome);
const router = new CommandRouter(chrome, tabs);
const connection = new BridgeConnection(chrome, identity, (context, command) =>
  router.execute(context, command),
);

chrome.debugger.onEvent.addListener((source, method, params) => {
  if (source.tabId !== undefined)
    tabs.byChromeTab(source.tabId)?.debuggee.handleEvent(source, method, params);
});
chrome.debugger.onDetach.addListener((source) => {
  if (source.tabId !== undefined) tabs.byChromeTab(source.tabId)?.debuggee.handleDetach();
});
chrome.tabs.onRemoved.addListener((tabId) => tabs.handleRemoved(tabId));
chrome.tabs.onUpdated.addListener((tabId, change) => tabs.handleUpdated(tabId, change));

// The open native port keeps the service worker alive; the alarm only
// restarts the connection after the worker was stopped anyway.
void chrome.alarms.create(KEEPALIVE_ALARM, { periodInMinutes: 1 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === KEEPALIVE_ALARM) void connection.connect();
});
void connection.connect();
