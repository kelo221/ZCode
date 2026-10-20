// 语言服务器进程与 JSON-RPC 通道（只实现诊断需要的子集）。
// 合同见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md「Client」。

import { spawn, type ChildProcess } from "node:child_process";
import { resolveExecutionCommand } from "../exec/execution-command.js";
import { terminateMcpStdioProcessTree } from "../mcp/process-tree.js";
import {
  attachProcessToWindowsJobObject,
  type WindowsJobObjectController,
} from "../mcp/windows-job-object.js";
import { encodeLspMessage, LspFramingError, LspMessageReader } from "./framing.js";

export const LSP_MAX_PENDING_REQUESTS = 64;
const METHOD_NOT_FOUND = -32601;
const REQUEST_FAILED = -32803;

export class LspRequestError extends Error {
  constructor(
    message: string,
    readonly code?: number,
  ) {
    super(message);
  }
}

export interface LspClientLogger {
  debug(message: string, context?: Record<string, unknown>): void;
  warn(message: string, context?: Record<string, unknown>): void;
}

export interface LspClientOptions {
  command: string;
  args: string[];
  cwd: string;
  env?: NodeJS.ProcessEnv;
  logger?: LspClientLogger;
  /** 测试注入；生产走 resolveExecutionCommand + child_process.spawn。 */
  spawnProcess?: (
    command: string,
    args: string[],
    cwd: string,
    env: NodeJS.ProcessEnv,
  ) => ChildProcess;
}

type RequestHandler = (params: unknown) => unknown;
type NotificationHandler = (params: unknown) => void;

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  cleanup: () => void;
}

export interface LspExitInfo {
  code: number | null;
  signal: NodeJS.Signals | null;
  reason?: string;
}

export class LspClient {
  private readonly reader = new LspMessageReader();
  private readonly pending = new Map<number, PendingRequest>();
  private readonly requestHandlers = new Map<string, RequestHandler>();
  private readonly notificationHandlers = new Map<string, NotificationHandler>();
  private readonly exitListeners: Array<(info: LspExitInfo) => void> = [];
  private nextId = 1;
  private exited?: LspExitInfo;
  private jobObject?: WindowsJobObjectController;

  private constructor(
    private readonly child: ChildProcess,
    private readonly logger?: LspClientLogger,
  ) {}

  static start(options: LspClientOptions): LspClient {
    const env = options.env ?? process.env;
    const child = options.spawnProcess
      ? options.spawnProcess(options.command, options.args, options.cwd, env)
      : spawnServer(options.command, options.args, options.cwd, env);
    const client = new LspClient(child, options.logger);
    client.wire();
    return client;
  }

  get alive(): boolean {
    return this.exited === undefined;
  }

  get pid(): number | undefined {
    return this.child.pid;
  }

  onExit(listener: (info: LspExitInfo) => void): void {
    if (this.exited) listener(this.exited);
    else this.exitListeners.push(listener);
  }

  onNotification(method: string, handler: NotificationHandler): void {
    this.notificationHandlers.set(method, handler);
  }

  onRequest(method: string, handler: RequestHandler): void {
    this.requestHandlers.set(method, handler);
  }

  notify(method: string, params: unknown): void {
    this.write({ jsonrpc: "2.0", method, params });
  }

  request<T>(
    method: string,
    params: unknown,
    options: { deadline?: number; signal?: AbortSignal } = {},
  ): Promise<T> {
    if (this.exited) return Promise.reject(new LspRequestError("language server exited"));
    if (this.pending.size >= LSP_MAX_PENDING_REQUESTS) {
      return Promise.reject(new LspRequestError("too many pending language server requests"));
    }
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const onAbort = () => {
        this.cancel(id);
        finish(() => reject(new LspRequestError("request aborted")));
      };
      const cleanup = () => {
        if (timer) clearTimeout(timer);
        options.signal?.removeEventListener("abort", onAbort);
        this.pending.delete(id);
      };
      const finish = (settle: () => void) => {
        cleanup();
        settle();
      };
      if (options.deadline !== undefined) {
        timer = setTimeout(
          () => {
            this.cancel(id);
            finish(() => reject(new LspRequestError("request timed out")));
          },
          Math.max(0, options.deadline - Date.now()),
        );
      }
      options.signal?.addEventListener("abort", onAbort, { once: true });
      this.pending.set(id, {
        resolve: (value) => finish(() => resolve(value as T)),
        reject: (error) => finish(() => reject(error)),
        cleanup,
      });
      this.write({ jsonrpc: "2.0", id, method, params });
    });
  }

  /** shutdown → exit，超时或失败后杀进程树。 */
  async close(timeoutMs = 3_000): Promise<void> {
    if (this.exited) return;
    try {
      await this.request("shutdown", null, { deadline: Date.now() + timeoutMs });
      this.notify("exit", null);
    } catch {
      // 服务器无响应时直接杀进程树。
    }
    await waitForExit(this, timeoutMs);
    await this.kill();
  }

  async kill(): Promise<void> {
    const pid = this.child.pid;
    try {
      this.jobObject?.terminate();
    } catch {
      // 退回 taskkill / 信号。
    }
    if (this.exited === undefined && typeof pid === "number" && pid > 0) {
      await terminateMcpStdioProcessTree(pid).catch(() => undefined);
    }
    this.jobObject?.close();
    this.jobObject = undefined;
  }

  private wire(): void {
    const pid = this.child.pid;
    if (typeof pid === "number") {
      void attachProcessToWindowsJobObject(pid).then((job) => {
        if (this.exited) job?.close();
        else this.jobObject = job;
      });
    }
    this.child.stdout?.on("data", (chunk: Buffer) => this.onData(chunk));
    this.child.stderr?.on("data", (chunk: Buffer) => {
      this.logger?.debug("Language server stderr", {
        event: "lsp.stderr",
        text: chunk.toString("utf8").slice(0, 2_000),
      });
    });
    this.child.stdin?.on("error", () => undefined);
    this.child.on("error", (error) =>
      this.markExited({ code: null, signal: null, reason: error.message }),
    );
    this.child.on("exit", (code, signal) => this.markExited({ code, signal }));
  }

  private onData(chunk: Buffer): void {
    let messages: unknown[];
    try {
      messages = this.reader.push(chunk);
    } catch (error) {
      const reason = error instanceof LspFramingError ? error.message : String(error);
      this.logger?.warn("Language server sent a malformed message", {
        event: "lsp.framing",
        reason,
      });
      this.markExited({ code: null, signal: null, reason });
      void this.kill();
      return;
    }
    for (const message of messages) this.dispatch(message);
  }

  private dispatch(message: unknown): void {
    if (!isRecord(message)) return;
    const id = message.id;
    const method = typeof message.method === "string" ? message.method : undefined;
    if (method !== undefined && (typeof id === "number" || typeof id === "string")) {
      this.answer(id, method, message.params);
      return;
    }
    if (method !== undefined) {
      try {
        this.notificationHandlers.get(method)?.(message.params);
      } catch (error) {
        this.logger?.warn("Language server notification handler failed", {
          event: "lsp.notification_failed",
          method,
          error: String(error),
        });
      }
      return;
    }
    if (typeof id !== "number") return;
    const pending = this.pending.get(id);
    if (!pending) return;
    if (isRecord(message.error)) {
      const code = typeof message.error.code === "number" ? message.error.code : undefined;
      pending.reject(new LspRequestError(String(message.error.message ?? "request failed"), code));
    } else {
      pending.resolve(message.result);
    }
  }

  private answer(id: number | string, method: string, params: unknown): void {
    const handler = this.requestHandlers.get(method);
    if (!handler) {
      this.write({
        jsonrpc: "2.0",
        id,
        error: { code: METHOD_NOT_FOUND, message: `Unsupported: ${method}` },
      });
      return;
    }
    try {
      this.write({ jsonrpc: "2.0", id, result: handler(params) ?? null });
    } catch (error) {
      this.write({ jsonrpc: "2.0", id, error: { code: REQUEST_FAILED, message: String(error) } });
    }
  }

  private cancel(id: number): void {
    if (this.alive) this.notify("$/cancelRequest", { id });
  }

  private write(message: unknown): void {
    if (this.exited || !this.child.stdin || this.child.stdin.destroyed) return;
    this.child.stdin.write(encodeLspMessage(message));
  }

  private markExited(info: LspExitInfo): void {
    if (this.exited) return;
    this.exited = info;
    for (const pending of [...this.pending.values()]) {
      pending.reject(new LspRequestError(info.reason ?? "language server exited"));
    }
    for (const listener of this.exitListeners.splice(0)) listener(info);
  }
}

function spawnServer(
  command: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
): ChildProcess {
  const resolved = resolveExecutionCommand({ mode: "argv", file: command, args }, { cwd, env });
  return spawn(resolved.file, resolved.args, {
    cwd,
    env,
    shell: resolved.shell,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
}

function waitForExit(client: LspClient, timeoutMs: number): Promise<void> {
  if (!client.alive) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, timeoutMs);
    client.onExit(() => {
      clearTimeout(timer);
      resolve();
    });
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
