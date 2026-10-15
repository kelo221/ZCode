import type { BrowserCommand, BrowserCommandResult } from "@zcode/shared";
import type { ChromeApi, ChromePort } from "./chromeApi.js";
import type { ExecutionContext } from "./commandRouter.js";

/** Registered by GPUI (Settings › Browser Use); must match `native.rs`. */
export const HOST_NAME = "dev.zcode.gpui.browser";

const RETRY_MIN_MS = 2_000;
const RETRY_MAX_MS = 60_000;

export interface BridgeIdentity {
  profileId: string;
  browser: string;
  profile: string;
  extensionVersion: string;
}

export interface BridgeStatus {
  connected: boolean;
  browserId?: string;
  generation?: number;
  /** Why the last attempt ended, e.g. `gpui_not_running`. */
  lastError?: string;
}

interface ExecuteMessage {
  op: "execute";
  requestId: string;
  command: BrowserCommand;
  context: Record<string, unknown>;
}

type Execute = (
  context: ExecutionContext,
  command: BrowserCommand,
) => Promise<BrowserCommandResult>;

function text(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

/**
 * The native messaging port to `zcode-browser-host`. It says hello, waits for
 * GPUI's welcome (browser id + generation), runs `execute` messages and
 * reconnects with backoff when GPUI is not running.
 */
export class BridgeConnection {
  private port: ChromePort | undefined;
  private welcome: { browserId: string; generation: number } | undefined;
  private retryMs = RETRY_MIN_MS;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private lastError: string | undefined;

  constructor(
    private readonly chrome: ChromeApi,
    private readonly identity: () => Promise<BridgeIdentity>,
    private readonly execute: Execute,
    private readonly onStatus: (status: BridgeStatus) => void = () => {},
  ) {}

  status(): BridgeStatus {
    return {
      connected: this.welcome !== undefined,
      ...this.welcome,
      ...(this.lastError && { lastError: this.lastError }),
    };
  }

  /** Connect unless a port is already open; safe to call from any wake-up. */
  async connect(): Promise<void> {
    if (this.port) return;
    clearTimeout(this.retryTimer);
    this.retryTimer = undefined;
    let port: ChromePort;
    try {
      port = this.chrome.runtime.connectNative(HOST_NAME);
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
      this.scheduleRetry();
      return;
    }
    this.port = port;
    port.onMessage.addListener((message) => void this.handle(port, message));
    port.onDisconnect.addListener(() => {
      if (this.port !== port) return;
      this.lastError ??= this.chrome.runtime.lastError?.message ?? "disconnected";
      this.port = undefined;
      this.welcome = undefined;
      this.onStatus(this.status());
      this.scheduleRetry();
    });
    port.postMessage({ op: "hello", ...(await this.identity()) });
  }

  private scheduleRetry(): void {
    if (this.retryTimer) return;
    const delay = this.retryMs;
    this.retryMs = Math.min(this.retryMs * 2, RETRY_MAX_MS);
    this.retryTimer = setTimeout(() => {
      this.retryTimer = undefined;
      void this.connect();
    }, delay);
  }

  private async handle(port: ChromePort, raw: unknown): Promise<void> {
    const message = (raw ?? {}) as Record<string, unknown>;
    switch (message.op) {
      case "welcome":
        this.welcome = {
          browserId: String(message.browserId),
          generation: Number(message.generation),
        };
        this.retryMs = RETRY_MIN_MS;
        this.lastError = undefined;
        this.onStatus(this.status());
        return;
      case "unavailable":
        this.lastError = text(message.reason) ?? "unavailable";
        return;
      case "execute":
        await this.run(port, message as unknown as ExecuteMessage);
        return;
    }
  }

  private async run(port: ChromePort, message: ExecuteMessage): Promise<void> {
    const welcome = this.welcome;
    if (!welcome) return;
    const c = message.context ?? {};
    const sessionId = text(c.sessionId) ?? "default";
    const context: ExecutionContext = {
      requestId: message.requestId,
      ...(text(c.turnId) ? { turnId: text(c.turnId) } : {}),
      sessionId,
      workspaceKey: text(c.workspaceKey) ?? sessionId,
      remoteSessionId: text(c.remoteSessionId) ?? "",
      browserId: welcome.browserId,
      generation: welcome.generation,
    };
    let result: BrowserCommandResult;
    try {
      result = await this.execute(context, message.command);
    } catch (error) {
      result = {
        ok: false,
        error: {
          code: "execution_error",
          message: error instanceof Error ? error.message : String(error),
          sideEffect: "uncertain",
        },
        elapsedMs: 0,
      };
    }
    // A reconnect in between means GPUI has already failed this request.
    if (this.port === port)
      port.postMessage({ op: "result", requestId: message.requestId, result });
  }
}
