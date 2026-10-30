import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { McpPort, McpServerConfig, McpToolDescriptor } from "@zcode/contracts";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createFileMcpToolCache,
  createLazyStdioMcpPort,
  isLazyStdioServer,
} from "../src/mcp/lazy-stdio.js";

let configSeq = 0;
// 探测结果按配置在进程内共享；每个用例用独立配置，避免用例间互相命中。
const plugin = (): McpServerConfig => ({
  type: "stdio",
  command: "bun",
  args: ["main.ts", "__zcode-plugin-host", `server-${++configSeq}.mjs`],
});
const user = (): McpServerConfig => ({
  type: "stdio",
  command: "uvx",
  args: [`user-mcp-${++configSeq}`],
});
const tool = (toolName: string, serverName = "docs"): McpToolDescriptor => ({
  serverName,
  toolName,
  inputSchema: { type: "object" },
});

function fakeInner(tools: McpToolDescriptor[]) {
  const connected = {
    status: "connected",
    toolCount: tools.length,
    transport: "stdio",
    updatedAt: "",
  };
  return {
    connectServer: vi.fn(async () => connected),
    connectConfiguredServers: vi.fn(),
    disconnectServer: vi.fn(async () => undefined),
    status: vi.fn(async () => ({ docs: connected })),
    listTools: vi.fn(async () => tools),
    callTool: vi.fn(async () => ({ content: [] })),
    close: vi.fn(async () => undefined),
  } as unknown as McpPort & {
    connectServer: ReturnType<typeof vi.fn>;
    disconnectServer: ReturnType<typeof vi.fn>;
  };
}

describe("lazy stdio MCP", () => {
  const roots: string[] = [];
  afterEach(async () => {
    // 工具缓存异步落盘，先等写入结束再删临时目录。
    await new Promise((r) => setTimeout(r, 50));
    vi.useRealTimers();
    await Promise.all(roots.splice(0).map((r) => rm(r, { recursive: true, force: true })));
  });
  const cacheFile = async () => {
    const root = await mkdtemp(join(tmpdir(), "zcode-lazy-mcp-"));
    roots.push(root);
    return join(root, "cache.json");
  };

  it("starts once to learn tools, then later sessions use the cache without a process", async () => {
    const config = plugin();
    const file = await cacheFile();
    const first = fakeInner([tool("read")]);
    const port1 = createLazyStdioMcpPort({ cache: createFileMcpToolCache(file), inner: first });
    await port1.connectServer("docs", config);
    expect(first.connectServer).toHaveBeenCalledTimes(1);
    await new Promise((r) => setTimeout(r, 50));

    const second = fakeInner([tool("read")]);
    const port2 = createLazyStdioMcpPort({ cache: createFileMcpToolCache(file), inner: second });
    const status = await port2.connectServer("docs", config);
    expect(status.status).toBe("connected");
    expect((await port2.listTools()).map((t) => t.toolName)).toEqual(["read"]);
    expect(second.connectServer).not.toHaveBeenCalled();

    await port2.callTool({ serverName: "docs", toolName: "read", arguments: {} });
    expect(second.connectServer).toHaveBeenCalledTimes(1);
  });

  it("stops an idle plugin server and restarts it on the next call", async () => {
    const config = plugin();
    const inner = fakeInner([tool("read")]);
    const port = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(await cacheFile()),
      idleStopMs: 20,
      inner,
    });
    await port.connectServer("docs", config);
    await port.callTool({ serverName: "docs", toolName: "read", arguments: {} });
    expect(inner.disconnectServer).toHaveBeenCalledTimes(1);
    await new Promise((r) => setTimeout(r, 60));
    expect(inner.disconnectServer).toHaveBeenCalledTimes(2);
    expect((await port.listTools()).length).toBe(1);
    await port.callTool({ serverName: "docs", toolName: "read", arguments: {} });
    expect(inner.connectServer).toHaveBeenCalledTimes(3);
  });

  it("stops a server started only to learn its tools, including user servers", async () => {
    const inner = fakeInner([tool("x", "blender")]);
    const port = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(await cacheFile()),
      inner,
    });
    const status = await port.connectServer("blender", user());
    await Promise.resolve();
    expect(status.status).toBe("connected");
    expect(inner.disconnectServer).toHaveBeenCalledWith("blender");
    expect((await port.listTools()).map((t) => t.toolName)).toEqual(["x"]);
  });

  it("shares one discovery between sessions that start together", async () => {
    const file = await cacheFile();
    const config = user();
    const inners = [
      fakeInner([tool("x", "shared")]),
      fakeInner([tool("x", "shared")]),
      fakeInner([tool("x", "shared")]),
    ];
    const ports = inners.map((inner) =>
      createLazyStdioMcpPort({ cache: createFileMcpToolCache(file), inner }),
    );
    await Promise.all(ports.map((port) => port.connectServer("shared", config)));
    const starts = inners.reduce((sum, inner) => sum + inner.connectServer.mock.calls.length, 0);
    expect(starts).toBe(1);
    for (const port of ports) expect((await port.listTools()).length).toBe(1);
  });

  it("keeps a server alive while a panel listens to its connection", async () => {
    const config = plugin();
    const inner = fakeInner([tool("read")]);
    const port = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(await cacheFile()),
      idleStopMs: 20,
      inner,
    });
    await port.connectServer("docs", config);
    const off = port.onAppConnectionInvalidated!("docs", () => {});
    await port.waitForAppConnection!("docs");
    inner.disconnectServer.mockClear();
    await new Promise((r) => setTimeout(r, 60));
    expect(inner.disconnectServer).not.toHaveBeenCalled();
    off();
    await new Promise((r) => setTimeout(r, 60));
    expect(inner.disconnectServer).toHaveBeenCalled();
  });

  it("never idle-stops a user server once it was used, and keeps node_repl eager", async () => {
    const inner = fakeInner([tool("x", "blender")]);
    const port = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(await cacheFile()),
      idleStopMs: 20,
      inner,
    });
    await port.connectServer("blender", user());
    await Promise.resolve();
    await port.callTool({ serverName: "blender", toolName: "x", arguments: {} });
    inner.disconnectServer.mockClear();
    await new Promise((r) => setTimeout(r, 60));
    expect(inner.disconnectServer).not.toHaveBeenCalled();
    expect(isLazyStdioServer("node_repl", plugin())).toBe(false);
    expect(isLazyStdioServer("blender", user())).toBe(true);
    expect(isLazyStdioServer("remote", { type: "http", url: "https://x" })).toBe(false);
  });

  it("bumps the tool revision when the real tool list differs from the cache", async () => {
    const config = plugin();
    const file = await cacheFile();
    const old = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(file),
      inner: fakeInner([tool("read")]),
    });
    await old.connectServer("docs", config);
    await new Promise((r) => setTimeout(r, 50));
    const port = createLazyStdioMcpPort({
      cache: createFileMcpToolCache(file),
      inner: fakeInner([tool("read"), tool("write")]),
    });
    await port.connectServer("docs", config);
    const before = port.toolListRevision!();
    await port.callTool({ serverName: "docs", toolName: "write", arguments: {} });
    expect(port.toolListRevision!()).toBeGreaterThan(before);
    expect((await port.listTools()).map((t) => t.toolName)).toEqual(["read", "write"]);
  });
});
