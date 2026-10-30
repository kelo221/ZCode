/**
 * 按需启动的 stdio MCP（fork 特性，docs: apps/zcode-gpui/PARITY.md「Lazy MCP servers」）。
 *
 * 原因：每个 stdio server 是常驻子进程（插件 host 约 70 MB），会话启动时全部拉起，
 * 即使整个会话一次都没调用。GPUI 多项目常开时仅插件 server 就占 2 GB 以上。
 *
 * 机制：工具清单按「server 名 + 配置哈希」持久化在磁盘缓存。命中缓存时会话启动只登记
 * 工具、不起进程；第一次 callTool / 资源读取 / 面板连接才真正启动。可安全回收的 server
 * 空闲一段时间后停掉进程、保留工具清单，下次调用再起。真实启动后若工具清单与缓存不同，
 * 递增 toolListRevision，运行时在下一个回合边界重新注册。
 */
import { createHash } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import type {
  Logger,
  McpCallToolOptions,
  McpCallToolRequest,
  McpConnectionSnapshot,
  McpConnectOptions,
  McpPort,
  McpServerConfig,
  McpServerStatus,
  McpToolCallResult,
  McpToolDescriptor,
} from "@zcode/contracts";
import { ZCODE_PLUGIN_HOST_COMMAND } from "@zcode/contracts";

export const LAZY_MCP_IDLE_STOP_MS = 5 * 60_000;

/** 进程内共享的工具清单探测（按缓存 key），成功结果保留到进程退出。 */
const discoveries = new Map<string, Promise<McpToolDescriptor[] | undefined>>();

/** node_repl 承载有状态 REPL 与浏览器 broker，必须常驻。 */
const ALWAYS_EAGER_SERVERS = new Set(["node_repl"]);

export function isLazyStdioServer(serverName: string, config: McpServerConfig): boolean {
  return config.type === "stdio" && !ALWAYS_EAGER_SERVERS.has(serverName);
}

/**
 * 只有插件 host 托管的官方插件 server 可空闲回收：它们按 workspace 共享且无会话状态。
 * 用户自配 server 可能持有状态（浏览器、REPL），只做延迟启动、不做回收。
 */
function canIdleStop(serverName: string, config: McpServerConfig): boolean {
  return (
    config.type === "stdio" &&
    !ALWAYS_EAGER_SERVERS.has(serverName) &&
    (config.args ?? []).includes(ZCODE_PLUGIN_HOST_COMMAND)
  );
}

export interface McpToolCache {
  get(key: string): Promise<McpToolDescriptor[] | undefined>;
  set(key: string, tools: McpToolDescriptor[]): void;
}

export function createFileMcpToolCache(file: string, logger?: Logger): McpToolCache {
  let loaded: Promise<Map<string, McpToolDescriptor[]>> | undefined;
  let writing: Promise<void> = Promise.resolve();
  const load = () =>
    (loaded ??= readFile(file, "utf8")
      .then((raw) => {
        const parsed = JSON.parse(raw) as { version?: number; entries?: unknown };
        if (parsed.version !== 1 || typeof parsed.entries !== "object" || !parsed.entries) {
          return new Map<string, McpToolDescriptor[]>();
        }
        return new Map(Object.entries(parsed.entries as Record<string, McpToolDescriptor[]>));
      })
      .catch(() => new Map<string, McpToolDescriptor[]>()));
  return {
    async get(key) {
      return (await load()).get(key);
    },
    set(key, tools) {
      writing = writing
        .then(async () => {
          const entries = await load();
          entries.set(key, tools);
          const temporary = `${file}.${process.pid}.tmp`;
          await mkdir(dirname(file), { recursive: true });
          await writeFile(
            temporary,
            JSON.stringify({ version: 1, entries: Object.fromEntries(entries) }),
          );
          await rename(temporary, file);
        })
        .catch((error) => {
          logger?.warn("MCP tool cache write failed", {
            error: error instanceof Error ? error.message : String(error),
            event: "mcp.lazy.cache_write_failed",
            module: "adapters.mcp.lazy",
          });
        });
    },
  };
}

export function lazyToolCacheKey(serverName: string, config: McpServerConfig): string {
  // 只存哈希：配置的 env 可能含凭据，不能落盘。
  const digest = createHash("sha256").update(stableStringify(config)).digest("hex");
  return `${serverName}\u0000${digest}`;
}

function toolsSignature(tools: readonly McpToolDescriptor[]): string {
  return stableStringify(
    tools.map((tool) => [tool.toolName, tool.description ?? "", tool.inputSchema]),
  );
}

export function createLazyStdioMcpPort(input: {
  cache: McpToolCache;
  idleStopMs?: number;
  inner: McpPort;
  logger?: Logger;
}): McpPort {
  const { cache, inner } = input;
  const idleStopMs = input.idleStopMs ?? LAZY_MCP_IDLE_STOP_MS;
  const logger = input.logger?.child({ module: "adapters.mcp.lazy" });
  let target: { name: string; config: McpServerConfig; options: McpConnectOptions } | undefined;
  let cachedTools: McpToolDescriptor[] | undefined;
  let running: Promise<McpServerStatus> | undefined;
  let revision = 0;
  let inFlight = 0;
  let pins = 0;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let closed = false;

  const cachedStatus = (): McpServerStatus => ({
    status: "connected",
    toolCount: cachedTools?.length ?? 0,
    transport: "stdio",
    updatedAt: new Date().toISOString(),
  });

  // 本次进程启动后是否真正被使用过（工具调用、资源、面板）。只为探测工具清单或设置页
  // 刷新而起的进程没有任何状态，答完立即停掉，否则用户 server 会一直常驻。
  let used = false;

  const stopNow = (event: string) => {
    if (!target || !running || inFlight > 0 || pins > 0 || closed) return;
    running = undefined;
    logger?.info("Lazy MCP server stopped", { event, mcpServerName: target.name });
    void inner.disconnectServer(target.name).catch(() => undefined);
  };

  const scheduleIdleStop = () => {
    if (idleTimer) clearTimeout(idleTimer);
    idleTimer = undefined;
    if (!target || !running || !canIdleStop(target.name, target.config)) return;
    idleTimer = setTimeout(() => {
      idleTimer = undefined;
      stopNow("mcp.lazy.idle_stopped");
    }, idleStopMs);
    idleTimer.unref?.();
  };

  const start = (reason: "probe" | "use"): Promise<McpServerStatus> => {
    if (!target) return Promise.reject(new Error("MCP server is not configured"));
    if (reason === "use") used = true;
    if (running) return running;
    used = reason === "use";
    const { name, config, options } = target;
    logger?.info("Lazy MCP server starting", {
      event: "mcp.lazy.started",
      mcpServerName: name,
      reason,
    });
    const startup = inner.connectServer(name, config, options).then(async (status) => {
      if (status.status === "connected") {
        const tools = (await inner.listTools()).filter((tool) => tool.serverName === name);
        if (!cachedTools || toolsSignature(cachedTools) !== toolsSignature(tools)) revision++;
        cachedTools = tools;
        cache.set(lazyToolCacheKey(name, config), tools);
      }
      return status;
    });
    running = startup;
    // 启动失败不保留 running，下次调用重新尝试。
    startup.catch(() => {
      if (running === startup) running = undefined;
    });
    void startup.then(
      () => (used ? scheduleIdleStop() : stopNow("mcp.lazy.probe_stopped")),
      () => undefined,
    );
    return startup;
  };

  const withServer = async <T>(work: () => Promise<T>): Promise<T> => {
    await start("use");
    inFlight++;
    try {
      return await work();
    } finally {
      inFlight--;
      scheduleIdleStop();
    }
  };

  const port: McpPort = {
    async connectServer(name, config, options = {}) {
      target = { name, config, options };
      if (options.revalidate) return await start("probe");
      const key = lazyToolCacheKey(name, config);
      cachedTools ??= await cache.get(key);
      if (running) return await running;
      if (cachedTools) return cachedStatus();
      // 同一 agent 里多个会话同时启动时共享一次探测，而不是每个会话各起一个进程。
      const shared = discoveries.get(key);
      if (shared) {
        const tools = await shared;
        if (tools) {
          cachedTools = tools;
          return cachedStatus();
        }
      }
      const startup = start("probe");
      const discovery = startup.then(
        (status) => (status.status === "connected" ? cachedTools : undefined),
        () => undefined,
      );
      discoveries.set(key, discovery);
      void discovery.then((tools) => {
        if (!tools && discoveries.get(key) === discovery) discoveries.delete(key);
      });
      return await startup;
    },
    async connectConfiguredServers(servers, options): Promise<McpConnectionSnapshot> {
      for (const [name, config] of Object.entries(servers)) {
        await port.connectServer(name, config, options);
      }
      return { statuses: await port.status(), tools: await port.listTools() };
    },
    async disconnectServer(name) {
      if (idleTimer) clearTimeout(idleTimer);
      running = undefined;
      return await inner.disconnectServer(name);
    },
    async pingServer(name, options) {
      if (!running) return true;
      return (await inner.pingServer?.(name, options)) ?? true;
    },
    async status() {
      if (!target) return {};
      if (running) return await inner.status();
      return { [target.name]: cachedStatus() };
    },
    async listTools() {
      const current = running;
      if (!current) return cachedTools ?? [];
      const status = await current.catch(() => undefined);
      return status?.status === "connected" ? await inner.listTools() : (cachedTools ?? []);
    },
    toolListRevision() {
      return revision + (running ? (inner.toolListRevision?.() ?? 0) : 0);
    },
    async callTool(request: McpCallToolRequest, options?: McpCallToolOptions) {
      return await withServer<McpToolCallResult>(() => inner.callTool(request, options));
    },
    async readResource(request, options) {
      return await withServer(() =>
        requireMethod(inner.readResource?.bind(inner))(request, options),
      );
    },
    async listResources(request, options) {
      return await withServer(() =>
        requireMethod(inner.listResources?.bind(inner))(request, options),
      );
    },
    async listResourceTemplates(request, options) {
      return await withServer(() =>
        requireMethod(inner.listResourceTemplates?.bind(inner))(request, options),
      );
    },
    async subscribeResource(request, options) {
      await withServer(() => requireMethod(inner.subscribeResource?.bind(inner))(request, options));
      pins++;
    },
    async unsubscribeResource(request, options) {
      if (!running || !inner.unsubscribeResource) return;
      await inner.unsubscribeResource(request, options);
      pins = Math.max(0, pins - 1);
      scheduleIdleStop();
    },
    async unsubscribeResourcesBySubscriber(prefix) {
      if (!running || !inner.unsubscribeResourcesBySubscriber) return 0;
      const released = await inner.unsubscribeResourcesBySubscriber(prefix);
      pins = Math.max(0, pins - released);
      scheduleIdleStop();
      return released;
    },
    // 面板绑定到具体连接代际；有面板监听时不回收，否则面板会收到「连接已变」。
    onAppConnectionInvalidated(name, listener) {
      pins++;
      const off = inner.onAppConnectionInvalidated?.(name, listener) ?? (() => {});
      let released = false;
      return () => {
        off();
        if (released) return;
        released = true;
        pins = Math.max(0, pins - 1);
        scheduleIdleStop();
      };
    },
    appConnectionSnapshot(name) {
      return running ? (inner.appConnectionSnapshot?.(name) ?? null) : null;
    },
    async waitForAppConnection(name) {
      await start("use");
      return inner.waitForAppConnection
        ? await inner.waitForAppConnection(name)
        : (inner.appConnectionSnapshot?.(name) ?? null);
    },
    async close() {
      closed = true;
      if (idleTimer) clearTimeout(idleTimer);
      running = undefined;
      await inner.close();
    },
  };
  return port;
}

function requireMethod<T extends (...args: never[]) => unknown>(method: T | undefined): T {
  if (!method) throw new Error("MCP adapter does not support resources");
  return method;
}

function stableStringify(value: unknown): string {
  if (value === null || typeof value !== "object") return JSON.stringify(value) ?? "null";
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  const record = value as Record<string, unknown>;
  return `{${Object.keys(record)
    .toSorted()
    .filter((key) => record[key] !== undefined)
    .map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`)
    .join(",")}}`;
}
