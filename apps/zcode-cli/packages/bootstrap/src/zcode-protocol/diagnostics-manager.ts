// 语言服务器诊断的进程级生命周期（GPUI 专属，见 apps/zcode-cli/docs/design/v2/tool/lsp-diagnostics.md）。
//
// 每个 (语言服务器, 项目根) 一个服务器进程，进程内所有会话、子代理与工作流子运行时共享。
// 最多 MAX 个，启动中的也计数；只驱逐空闲服务器（无等待者、未在启动、无检查进行中），
// 全部忙时请求在自己的截止时间内排队，到时返回 capacity。所属 workspace 没有活跃 Agent
// 且服务器空闲满 10 分钟后关闭；崩溃的服务器从表中移除，下一次请求重新启动。

import type {
  DiagnosticsMode,
  DiagnosticsPort,
  DiagnosticsRequest,
  FileDiagnosticsResult,
  Logger,
} from "@zcode/contracts";
import {
  findDiagnosticsRoot,
  findExecutableOnPath,
  filePathKey,
  LanguageServer,
  languageServerForFile,
  resolveCargoWorkspaceRoot,
  type CollectRequest,
  type LanguageServerSpec,
} from "@zcode/adapters";

export const DIAGNOSTICS_MAX_SERVERS = 4;
export const DIAGNOSTICS_IDLE_CLOSE_MS = 10 * 60_000;
export const DIAGNOSTICS_IDLE_POLL_MS = 15_000;
const CAPACITY_RECHECK_MS = 250;
const MISSING_EXECUTABLE_RECHECK_MS = 60_000;

export interface DiagnosticsServerHandle {
  readonly alive: boolean;
  isBusy(): boolean;
  collect(request: CollectRequest): Promise<FileDiagnosticsResult[]>;
  close(): Promise<void>;
  onExit(listener: () => void): void;
}

export interface DiagnosticsScope {
  workspaceKey: string;
}

export interface DiagnosticsManagerOptions {
  /** 该 workspace 是否还有活跃 Agent（turn、排队命令、后台任务、待答交互）。 */
  isWorkspaceBusy: (workspaceKey: string) => boolean;
  logger?: Logger;
  maxServers?: number;
  idleCloseMs?: number;
  pollMs?: number;
  now?: () => number;
  createServer?: (spec: LanguageServerSpec, root: string) => DiagnosticsServerHandle;
  findRoot?: (spec: LanguageServerSpec, filePath: string) => Promise<string | undefined>;
  isInstalled?: (command: string) => Promise<boolean>;
}

interface ServerEntry {
  key: string;
  spec: LanguageServerSpec;
  root: string;
  server: DiagnosticsServerHandle;
  waiters: number;
  workspaceKeys: Set<string>;
  lastUsed: number;
  idleSince?: number;
}

interface FileGroup {
  spec: LanguageServerSpec;
  root: string;
  files: string[];
}

export class DiagnosticsManager {
  private readonly entries = new Map<string, ServerEntry>();
  private readonly installed = new Map<string, { ok: boolean; checkedAt: number }>();
  private readonly cargoRoots = new Map<string, Promise<string | undefined>>();
  private readonly slotWaiters = new Set<() => void>();
  private poll?: ReturnType<typeof setInterval>;
  private closed = false;
  private readonly maxServers: number;
  private readonly idleCloseMs: number;
  private readonly pollMs: number;
  private readonly now: () => number;

  constructor(private readonly options: DiagnosticsManagerOptions) {
    this.maxServers = options.maxServers ?? DIAGNOSTICS_MAX_SERVERS;
    this.idleCloseMs = options.idleCloseMs ?? DIAGNOSTICS_IDLE_CLOSE_MS;
    this.pollMs = options.pollMs ?? DIAGNOSTICS_IDLE_POLL_MS;
    this.now = options.now ?? Date.now;
  }

  portFor(scope: DiagnosticsScope): DiagnosticsPort {
    return {
      collect: (request, options) => this.collect(scope, request, options?.signal),
    };
  }

  runningServers(): Array<{ server: string; root: string }> {
    return [...this.entries.values()].map((entry) => ({ server: entry.spec.id, root: entry.root }));
  }

  async close(): Promise<void> {
    this.closed = true;
    this.stopPoll();
    const servers = [...this.entries.values()].map((entry) => entry.server);
    this.entries.clear();
    this.wakeSlotWaiters();
    await Promise.allSettled(servers.map((server) => server.close()));
  }

  /** 每个轮询周期调用；导出供测试直接驱动。 */
  tick(): void {
    const now = this.now();
    for (const [key, entry] of this.entries) {
      if (this.entryBusy(entry) || [...entry.workspaceKeys].some((ws) => this.workspaceBusy(ws))) {
        entry.idleSince = undefined;
        continue;
      }
      entry.idleSince ??= now;
      if (now - entry.idleSince < this.idleCloseMs) continue;
      this.entries.delete(key);
      void entry.server.close();
      this.options.logger?.info("Language server closed after idle", {
        event: "zcode_protocol.diagnostics.idle_closed",
        idleMs: now - entry.idleSince,
        server: entry.spec.id,
      });
    }
    this.wakeSlotWaiters();
    if (this.entries.size === 0) this.stopPoll();
  }

  async collect(
    scope: DiagnosticsScope,
    request: DiagnosticsRequest,
    signal?: AbortSignal,
  ): Promise<FileDiagnosticsResult[]> {
    signal?.throwIfAborted();
    const files = dedupeFiles(request.files);
    const results = new Map<string, FileDiagnosticsResult>();
    const groups = new Map<string, FileGroup>();
    await Promise.all(
      files.map(async (file) => {
        const spec = languageServerForFile(file);
        if (!spec) {
          results.set(
            file,
            emptyResult(
              file,
              undefined,
              "unsupported",
              request.mode,
              "no language server handles this file type",
            ),
          );
          return;
        }
        if (this.closed) {
          results.set(
            file,
            emptyResult(file, spec.id, "unavailable", request.mode, "the agent is shutting down"),
          );
          return;
        }
        if (!(await this.isInstalled(spec.command))) {
          results.set(
            file,
            emptyResult(
              file,
              spec.id,
              "unavailable",
              request.mode,
              `${spec.command} is not installed (not found on PATH)`,
            ),
          );
          return;
        }
        const root = await this.findRoot(spec, file);
        if (!root) {
          results.set(
            file,
            emptyResult(
              file,
              spec.id,
              "unsupported",
              request.mode,
              "no project root was found for this file",
            ),
          );
          return;
        }
        const key = entryKey(spec, root);
        const group = groups.get(key) ?? { spec, root, files: [] };
        group.files.push(file);
        groups.set(key, group);
      }),
    );

    await Promise.all(
      [...groups.values()].map(async (group) => {
        for (const result of await this.collectGroup(scope, group, request, signal)) {
          results.set(result.filePath, result);
        }
      }),
    );
    signal?.throwIfAborted();
    return files.map(
      (file) =>
        results.get(file) ??
        emptyResult(file, undefined, "unavailable", request.mode, "no result was produced"),
    );
  }

  private async collectGroup(
    scope: DiagnosticsScope,
    group: FileGroup,
    request: DiagnosticsRequest,
    signal: AbortSignal | undefined,
  ): Promise<FileDiagnosticsResult[]> {
    const entry = await this.acquire(group.spec, group.root, request.deadline, signal);
    if (!entry) {
      const note = this.closed
        ? "the agent is shutting down"
        : `all ${this.maxServers} language servers are busy; try again shortly`;
      return group.files.map((file) =>
        emptyResult(
          file,
          group.spec.id,
          this.closed ? "unavailable" : "capacity",
          request.mode,
          note,
        ),
      );
    }
    entry.workspaceKeys.add(scope.workspaceKey);
    try {
      const results = await entry.server.collect({
        files: group.files,
        mode: request.mode,
        deadline: request.deadline,
        signal,
      });
      // 服务器返回的 filePath 与请求一致；按请求顺序映射，避免大小写/分隔符差异丢结果。
      return group.files.map((file, index) => ({
        ...(results[index] ??
          emptyResult(file, group.spec.id, "unavailable", request.mode, "no result was produced")),
        filePath: file,
      }));
    } catch (error) {
      if (signal?.aborted) throw error;
      const message = error instanceof Error ? error.message : String(error);
      this.options.logger?.warn("Diagnostics collection failed", {
        error: message,
        event: "zcode_protocol.diagnostics.failed",
        server: group.spec.id,
      });
      return group.files.map((file) =>
        emptyResult(file, group.spec.id, "unavailable", request.mode, message.slice(0, 400)),
      );
    } finally {
      // 只减少等待者：别的等待者共享的检查继续进行。
      entry.waiters -= 1;
      entry.lastUsed = this.now();
      this.wakeSlotWaiters();
    }
  }

  /** 拿到一个已登记等待者的服务器；容量不足且截止前无空位时返回 undefined。 */
  private async acquire(
    spec: LanguageServerSpec,
    root: string,
    deadline: number,
    signal: AbortSignal | undefined,
  ): Promise<ServerEntry | undefined> {
    const key = entryKey(spec, root);
    for (;;) {
      signal?.throwIfAborted();
      if (this.closed) return undefined;
      const existing = this.entries.get(key);
      if (existing && existing.server.alive) {
        existing.waiters += 1;
        existing.idleSince = undefined;
        return existing;
      }
      if (existing) this.entries.delete(key);
      if (this.entries.size >= this.maxServers) this.evictOneIdle();
      if (this.entries.size < this.maxServers) {
        const entry = this.start(spec, root, key);
        entry.waiters += 1;
        return entry;
      }
      // 截止时间是请求方的墙钟时间（epoch ms）；this.now 只用于空闲计时。
      if (Date.now() >= deadline) return undefined;
      await this.waitForSlot(deadline, signal);
    }
  }

  private start(spec: LanguageServerSpec, root: string, key: string): ServerEntry {
    const server = this.options.createServer
      ? this.options.createServer(spec, root)
      : new LanguageServer({ spec, root, logger: this.options.logger });
    const entry: ServerEntry = {
      key,
      spec,
      root,
      server,
      waiters: 0,
      workspaceKeys: new Set(),
      lastUsed: this.now(),
    };
    server.onExit(() => {
      if (this.entries.get(key) === entry) this.entries.delete(key);
      this.wakeSlotWaiters();
    });
    this.entries.set(key, entry);
    this.startPoll();
    this.options.logger?.info("Language server started", {
      event: "zcode_protocol.diagnostics.started",
      server: spec.id,
    });
    return entry;
  }

  private evictOneIdle(): void {
    let victim: ServerEntry | undefined;
    for (const entry of this.entries.values()) {
      if (this.entryBusy(entry)) continue;
      if (!victim || entry.lastUsed < victim.lastUsed) victim = entry;
    }
    if (!victim) return;
    this.entries.delete(victim.key);
    void victim.server.close();
    this.options.logger?.info("Language server evicted for capacity", {
      event: "zcode_protocol.diagnostics.evicted",
      server: victim.spec.id,
    });
  }

  private entryBusy(entry: ServerEntry): boolean {
    if (entry.waiters > 0) return true;
    try {
      return entry.server.isBusy();
    } catch {
      return true;
    }
  }

  private waitForSlot(deadline: number, signal: AbortSignal | undefined): Promise<void> {
    return new Promise((resolve) => {
      // 检查结束不会通知管理器，因此除了等待者释放外还要定期重查。
      const wait = Math.max(0, Math.min(CAPACITY_RECHECK_MS, deadline - Date.now()));
      const done = (): void => {
        clearTimeout(timer);
        this.slotWaiters.delete(done);
        signal?.removeEventListener("abort", done);
        resolve();
      };
      const timer = setTimeout(done, wait);
      this.slotWaiters.add(done);
      signal?.addEventListener("abort", done, { once: true });
    });
  }

  private wakeSlotWaiters(): void {
    for (const wake of [...this.slotWaiters]) wake();
  }

  private async isInstalled(command: string): Promise<boolean> {
    const cached = this.installed.get(command);
    const now = this.now();
    if (cached && (cached.ok || now - cached.checkedAt < MISSING_EXECUTABLE_RECHECK_MS))
      return cached.ok;
    const ok = this.options.isInstalled
      ? await this.options.isInstalled(command)
      : (await findExecutableOnPath(command)) !== undefined;
    this.installed.set(command, { ok, checkedAt: now });
    return ok;
  }

  private findRoot(spec: LanguageServerSpec, file: string): Promise<string | undefined> {
    if (this.options.findRoot) return this.options.findRoot(spec, file);
    return findDiagnosticsRoot(spec, file, {
      cargoWorkspaceRoot: (manifest) => {
        const cacheKey = filePathKey(manifest);
        let located = this.cargoRoots.get(cacheKey);
        if (!located) {
          located = resolveCargoWorkspaceRoot(manifest);
          this.cargoRoots.set(cacheKey, located);
        }
        return located;
      },
    });
  }

  private workspaceBusy(workspaceKey: string): boolean {
    try {
      return this.options.isWorkspaceBusy(workspaceKey);
    } catch {
      return true;
    }
  }

  private startPoll(): void {
    if (this.poll || this.closed) return;
    this.poll = setInterval(() => this.tick(), this.pollMs);
    this.poll.unref?.();
  }

  private stopPoll(): void {
    if (this.poll) clearInterval(this.poll);
    this.poll = undefined;
  }
}

function entryKey(spec: LanguageServerSpec, root: string): string {
  return `${spec.id}\u0000${filePathKey(root)}`;
}

function dedupeFiles(files: string[]): string[] {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const file of files) {
    const key = filePathKey(file);
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(file);
  }
  return unique;
}

function emptyResult(
  filePath: string,
  server: string | undefined,
  availability: FileDiagnosticsResult["availability"],
  mode: DiagnosticsMode,
  note: string,
): FileDiagnosticsResult {
  return {
    filePath,
    ...(server ? { server } : {}),
    availability,
    freshness: "best_effort",
    check:
      mode === "quick"
        ? "not_requested"
        : availability === "unsupported"
          ? "not_supported"
          : "incomplete",
    timedOut: availability === "capacity",
    errorCount: 0,
    warningCount: 0,
    diagnostics: [],
    knownOtherFileErrors: 0,
    note,
  };
}
