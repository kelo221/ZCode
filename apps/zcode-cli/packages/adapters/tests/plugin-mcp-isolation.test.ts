import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { createNodePluginAdapter } from "../src/plugins/index.js";

const PLUGIN_NAME = "isolation-plugin";
const TRANSPORTS = {
  stdio: { type: "stdio", command: "node" },
  http: { type: "http", url: "https://mcp.example.test/mcp" },
};

describe.each(["file", "manifest"] as const)("plugin MCP isolation from %s", (source) => {
  it.each([
    { label: "workspace", input: "workspace", expected: "workspace" },
    { label: "session", input: "session", expected: "session" },
    { label: "omitted", input: undefined, expected: undefined },
    { label: "unknown value", input: "global", expected: undefined },
  ])("carries the declared isolation ($label) into the server config", async ({ input, expected }) => {
    const root = await mkdtemp(join(tmpdir(), "zcode-plugin-isolation-"));
    const pluginRoot = join(root, PLUGIN_NAME);
    const servers = Object.fromEntries(
      Object.entries(TRANSPORTS).map(([name, transport]) => [
        name,
        { ...transport, isolation: input },
      ]),
    );
    try {
      await mkdir(join(pluginRoot, ".zcode-plugin"), { recursive: true });
      await writeFile(
        join(pluginRoot, ".zcode-plugin", "plugin.json"),
        JSON.stringify({
          name: PLUGIN_NAME,
          ...(source === "manifest" ? { mcpServers: servers } : {}),
        }),
      );
      if (source === "file") {
        await writeFile(join(pluginRoot, ".mcp.json"), JSON.stringify({ mcpServers: servers }));
      }
      const storageRoot = join(root, "storage");
      const outcome = await createNodePluginAdapter({ storageRoot }).discoverPlugins({
        config: { dirs: [pluginRoot], enabled: true, enabledPlugins: {}, options: {} },
        env: {},
        storageRoot,
        workingDirectory: root,
      });
      expect(outcome.diagnostics).toEqual([]);
      for (const name of Object.keys(TRANSPORTS)) {
        expect(outcome.mcpServers[`plugin:${PLUGIN_NAME}:${name}`]?.isolation, name).toBe(
          expected,
        );
      }
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
