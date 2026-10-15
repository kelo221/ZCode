import type { ChromeApi, DebuggeeTarget } from "./chromeApi.js";

type Listener = (method: string, params: Record<string, unknown>, sessionId?: string) => void;

interface ChildTarget {
  sessionId: string;
  targetInfo: Record<string, unknown> & { targetId: string };
}

const PROTOCOL_VERSION = "1.3";
const CHILD_REGISTRATION_TIMEOUT_MS = 1_000;

/**
 * One `chrome.debugger` attachment to a tab. The command layer was written
 * for Electron's debugger, which may call `Target.attachToTarget`; extensions
 * may not. Cross-origin iframes are auto-attached here instead, and the
 * `Target.*` calls the command layer makes are answered from those sessions.
 */
export class TabDebugger {
  private attaching: Promise<void> | undefined;
  private attached = false;
  private readonly children = new Map<string, ChildTarget>();
  private readonly listeners = new Set<Listener>();
  private pendingCommands = 0;

  constructor(
    private readonly chrome: ChromeApi,
    readonly chromeTabId: number,
  ) {}

  get isAttached(): boolean {
    return this.attached;
  }

  get busy(): boolean {
    return this.pendingCommands > 0;
  }

  onEvent(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async ensureAttached(): Promise<void> {
    if (this.attached) return;
    this.attaching ??= (async () => {
      const root = { tabId: this.chromeTabId };
      try {
        await this.chrome.debugger.attach(root, PROTOCOL_VERSION);
      } catch (error) {
        // Another debugger client (or our own stale attach) may hold the tab.
        if (!String(error).includes("Another debugger is already attached")) throw error;
      }
      this.attached = true;
      await this.chrome.debugger.sendCommand(root, "Page.enable");
      // 修复原因：受控 tab 在后台打开，visibilityState=hidden 时渲染进程不处理输入，
      // 每个 Input.dispatchMouseEvent 卡约 5 秒且 mousedown/click 丢失（Helium 实测）。
      // 焦点模拟让 Chrome 把该 tab 当作可见且有焦点（与 Playwright 保持后台页可用的做法相同），
      // debugger detach（轮次结束）时自动撤销，用户前台 tab 不受影响。
      await this.chrome.debugger.sendCommand(root, "Emulation.setFocusEmulationEnabled", {
        enabled: true,
      });
      await this.chrome.debugger.sendCommand(root, "Target.setAutoAttach", {
        autoAttach: true,
        waitForDebuggerOnStart: false,
        flatten: true,
      });
    })().finally(() => {
      this.attaching = undefined;
    });
    await this.attaching;
  }

  async send(method: string, params?: unknown, sessionId?: string): Promise<unknown> {
    await this.ensureAttached();
    switch (method) {
      case "Target.attachToTarget":
        return { sessionId: await this.childSession(params) };
      case "Target.getTargets":
        return { targetInfos: [...this.children.values()].map((c) => c.targetInfo) };
      case "Target.detachFromTarget":
        // Auto-attached sessions stay for the life of the frame.
        return {};
    }
    const target: DebuggeeTarget = { tabId: this.chromeTabId, ...(sessionId ? { sessionId } : {}) };
    this.pendingCommands += 1;
    try {
      return await this.chrome.debugger.sendCommand(target, method, (params ?? {}) as object);
    } finally {
      this.pendingCommands -= 1;
    }
  }

  private async childSession(params: unknown): Promise<string> {
    const targetId = (params as { targetId?: unknown } | undefined)?.targetId;
    if (typeof targetId !== "string") throw new Error("Target.attachToTarget needs a targetId");
    const deadline = Date.now() + CHILD_REGISTRATION_TIMEOUT_MS;
    while (Date.now() < deadline) {
      const child = this.children.get(targetId);
      if (child) return child.sessionId;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error(`No target with given id found: ${targetId}`);
  }

  /** Called by the extension-wide `chrome.debugger.onEvent` router. */
  handleEvent(source: DebuggeeTarget, method: string, raw?: unknown): void {
    const params = (raw ?? {}) as Record<string, unknown>;
    if (method === "Target.attachedToTarget") {
      const sessionId = params.sessionId as string;
      const targetInfo = params.targetInfo as ChildTarget["targetInfo"];
      this.children.set(targetInfo.targetId, { sessionId, targetInfo });
      // Nested out-of-process frames attach through their parent session.
      void this.chrome.debugger
        .sendCommand({ tabId: this.chromeTabId, sessionId }, "Target.setAutoAttach", {
          autoAttach: true,
          waitForDebuggerOnStart: false,
          flatten: true,
        })
        .catch(() => undefined);
    } else if (method === "Target.detachedFromTarget") {
      for (const [targetId, child] of this.children) {
        if (child.sessionId === params.sessionId) this.children.delete(targetId);
      }
    }
    for (const listener of this.listeners) listener(method, params, source.sessionId);
  }

  /** The browser dropped the attachment (tab closed, user cancelled the banner). */
  handleDetach(): void {
    this.attached = false;
    this.children.clear();
  }

  async detach(): Promise<void> {
    if (!this.attached) return;
    this.handleDetach();
    await this.chrome.debugger.detach({ tabId: this.chromeTabId }).catch(() => undefined);
  }
}
