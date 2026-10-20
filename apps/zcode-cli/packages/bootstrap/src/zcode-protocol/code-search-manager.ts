// fff 常驻索引的进程级生命周期（GPUI 专属，见 apps/zcode-gpui/PARITY.md「Built-in fff code search」）。
//
// 一个 workspace 一份索引，进程内所有会话、子代理与工作流子运行时共享。首次查询时打开；
// workspace 没有任何活跃 Agent 满 60 秒后关闭（轮询判定，不依赖 turn 事件顺序：
// TurnComplete 落库早于 activeTurn 清除），下次查询再打开。

import type { CodeSearchOutcome, CodeSearchPort, Logger } from "@zcode/contracts";
import type { CodeSearchEngine, CodeSearchEngineLoad } from "@zcode/adapters";

export const CODE_SEARCH_IDLE_CLOSE_MS = 60_000;
export const CODE_SEARCH_IDLE_POLL_MS = 5_000;

export interface CodeSearchScope {
  workspaceKey: string;
  root: string;
  /** 空闲判定用的 workspace；worktree 子代理的索引跟随父 workspace 的忙闲。 */
  busyKey?: string;
}

export interface CodeSearchManagerOptions {
  load: () => Promise<CodeSearchEngineLoad>;
  /** 该 workspace 是否还有活跃 Agent（turn、排队命令、后台任务、待答交互）。 */
  isWorkspaceBusy: (workspaceKey: string) => boolean;
  logger?: Logger;
  idleCloseMs?: number;
  pollMs?: number;
  now?: () => number;
}

interface IndexEntry {
  root: string;
  busyKey?: string;
  engine?: CodeSearchEngine;
  opening?: Promise<CodeSearchEngine>;
  inFlight: number;
  idleSince?: number;
}

export class CodeSearchManager {
  private readonly entries = new Map<string, IndexEntry>();
  private loadPromise?: Promise<CodeSearchEngineLoad>;
  private poll?: ReturnType<typeof setInterval>;
  private closed = false;
  private readonly idleCloseMs: number;
  private readonly pollMs: number;
  private readonly now: () => number;

  constructor(private readonly options: CodeSearchManagerOptions) {
    this.idleCloseMs = options.idleCloseMs ?? CODE_SEARCH_IDLE_CLOSE_MS;
    this.pollMs = options.pollMs ?? CODE_SEARCH_IDLE_POLL_MS;
    this.now = options.now ?? Date.now;
  }

  /** 原生库只加载一次；失败后整个进程都不注册这两个工具，Agent 回退到 Bash rg。 */
  async available(): Promise<boolean> {
    this.loadPromise ??= this.options.load().then((load) => {
      if (!load.ok) {
        this.options.logger?.warn("Code search unavailable", {
          error: load.error,
          event: "zcode_protocol.code_search.unavailable",
        });
      }
      return load;
    });
    return (await this.loadPromise).ok;
  }

  portFor(scope: CodeSearchScope): CodeSearchPort {
    return {
      findFiles: (request, options) =>
        this.run(scope, options?.signal, (engine) => engine.find(request)),
      searchCode: (request, options) =>
        this.run(scope, options?.signal, (engine) => engine.search(request)),
    };
  }

  openWorkspaceKeys(): string[] {
    return [...this.entries.entries()]
      .filter(([, entry]) => entry.engine !== undefined)
      .map(([key]) => key);
  }

  close(): void {
    this.closed = true;
    this.stopPoll();
    for (const entry of this.entries.values()) entry.engine?.destroy();
    this.entries.clear();
  }

  /** 每个轮询周期调用；导出供测试直接驱动。 */
  tick(): void {
    const now = this.now();
    for (const [key, entry] of this.entries) {
      if (entry.inFlight > 0 || this.busy(entry.busyKey ?? key)) {
        entry.idleSince = undefined;
        continue;
      }
      entry.idleSince ??= now;
      if (now - entry.idleSince < this.idleCloseMs) continue;
      this.entries.delete(key);
      entry.engine?.destroy();
      this.options.logger?.info("Code search index closed after idle", {
        event: "zcode_protocol.code_search.idle_closed",
        idleMs: now - entry.idleSince,
      });
    }
    if (this.entries.size === 0) this.stopPoll();
  }

  private async run<T>(
    scope: CodeSearchScope,
    signal: AbortSignal | undefined,
    query: (engine: CodeSearchEngine) => CodeSearchOutcome<T>,
  ): Promise<CodeSearchOutcome<T>> {
    signal?.throwIfAborted();
    if (this.closed) return unavailable("the agent is shutting down");
    const load = await this.loadPromise;
    if (!load?.ok) return unavailable(load?.error ?? "fff is not loaded");

    let entry = this.entries.get(scope.workspaceKey);
    if (entry === undefined) {
      entry = {
        root: scope.root,
        inFlight: 0,
        ...(scope.busyKey ? { busyKey: scope.busyKey } : {}),
      };
      this.entries.set(scope.workspaceKey, entry);
    }
    entry.inFlight += 1;
    entry.idleSince = undefined;
    this.startPoll();
    try {
      const engine = await this.ensureEngine(scope.workspaceKey, entry, load.open);
      signal?.throwIfAborted();
      return query(engine);
    } catch (error) {
      if (signal?.aborted) throw error;
      const message = error instanceof Error ? error.message : String(error);
      this.options.logger?.warn("Code search failed", {
        error: message,
        event: "zcode_protocol.code_search.failed",
      });
      return unavailable(message);
    } finally {
      entry.inFlight -= 1;
    }
  }

  private async ensureEngine(
    workspaceKey: string,
    entry: IndexEntry,
    open: (root: string) => Promise<CodeSearchEngine>,
  ): Promise<CodeSearchEngine> {
    if (entry.engine) return entry.engine;
    if (!entry.opening) {
      const startedAt = this.now();
      entry.opening = open(entry.root).then(
        (engine) => {
          entry.opening = undefined;
          if (this.closed || this.entries.get(workspaceKey) !== entry) {
            engine.destroy();
            throw new Error("the index was closed while opening");
          }
          entry.engine = engine;
          this.options.logger?.info("Code search index opened", {
            durationMs: this.now() - startedAt,
            event: "zcode_protocol.code_search.opened",
          });
          return engine;
        },
        (error: unknown) => {
          entry.opening = undefined;
          // 打开失败（例如 workspace 是 home 目录）不缓存，下一次查询重试。
          if (this.entries.get(workspaceKey) === entry && entry.inFlight <= 1) {
            this.entries.delete(workspaceKey);
          }
          throw error;
        },
      );
    }
    return entry.opening;
  }

  private busy(workspaceKey: string): boolean {
    try {
      return this.options.isWorkspaceBusy(workspaceKey);
    } catch {
      // 判定失败按忙处理：宁可多留一分钟索引，也不在 turn 中途关掉。
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

function unavailable(message: string): CodeSearchOutcome<never> {
  return { ok: false, reason: "unavailable", message };
}
