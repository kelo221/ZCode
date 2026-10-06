import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { zcodePluginsListResultSchema, zcodePluginsConfigureResultSchema } from "@zcode/shared";

const pluginId = "config-smoke@inline";
export async function preparePluginConfigSmoke(scratch: string, workspace: string) {
  const plugins = join(scratch, "inline-plugins");
  const root = join(plugins, "config-smoke", ".zcode-plugin");
  const userDir = join(scratch, "home", ".zcode", "cli");
  const workspaceDir = join(workspace, ".zcode");
  await Promise.all([
    mkdir(root, { recursive: true }),
    mkdir(userDir, { recursive: true }),
    mkdir(workspaceDir, { recursive: true }),
  ]);
  await writeFile(
    join(root, "plugin.json"),
    JSON.stringify({
      name: "config-smoke",
      version: "0.1.0",
      userConfig: {
        endpoint: { type: "string", default: "fixture-default" },
        token: { type: "string", sensitive: true },
        count: { type: "number" },
        active: { type: "boolean" },
      },
    }),
    { flag: "wx" },
  );
  await writeFile(
    join(userDir, "config.json"),
    JSON.stringify({
      plugins: {
        dirs: [join(plugins, "config-smoke")],
        enabledPlugins: { [pluginId]: true },
        options: {
          [pluginId]: {
            endpoint: "user-endpoint",
            token: "user-fixture-secret",
            count: 2,
            active: true,
          },
        },
      },
    }),
    { flag: "wx" },
  );
  await writeFile(
    join(workspaceDir, "config.json"),
    JSON.stringify({
      plugins: {
        enabledPlugins: { [pluginId]: false },
        options: {
          [pluginId]: { endpoint: "workspace-endpoint", token: "workspace-fixture-secret" },
        },
      },
    }),
    { flag: "wx" },
  );
}
export async function backendPluginConfigSmoke(
  scratch: string,
  workspace: string,
  rpc: (method: string, params: unknown) => Promise<unknown>,
) {
  const ref = { workspacePath: workspace, workspaceKey: workspace };
  const userFile = join(scratch, "home", ".zcode", "cli", "config.json");
  const workspaceFile = join(workspace, ".zcode", "config.json");
  const read = async (path: string) =>
    JSON.parse(await readFile(path, "utf8")) as {
      plugins: {
        enabledPlugins?: Record<string, boolean>;
        options?: Record<string, Record<string, unknown>>;
      };
    };
  const list = async (scope: "user" | "workspace") => {
    const result = zcodePluginsListResultSchema.parse(
      await rpc("plugins/list", { workspace: ref, configScope: scope }),
    );
    if (result.diagnostics.some((d) => d.severity === "error"))
      throw new Error(
        `Disposable config list failed: ${result.diagnostics
          .filter((d) => d.severity === "error")
          .map((d) => `${d.code}:${d.pluginId ?? "global"}`)
          .join(",")}`,
      );
    const plugin = result.plugins.find((p) => p.id === pluginId);
    if (!plugin || plugin.configuredOptions?.token !== undefined)
      throw new Error("Scoped config missing or sensitive readback exposed");
    return plugin;
  };
  const mutate = async (method: string, scope: "user" | "workspace", params = {}) => {
    const result = zcodePluginsConfigureResultSchema.parse(
      await rpc(method, { workspace: ref, pluginId, scope, ...params }),
    );
    if (result.pluginId !== pluginId || result.diagnostics.some((d) => d.severity === "error"))
      throw new Error("Disposable config mutation failed");
  };
  const user = await list("user");
  const project = await list("workspace");
  if (
    !user.enabled ||
    user.enabledSource !== "user" ||
    user.configuredOptions?.endpoint !== "user-endpoint" ||
    user.optionSources?.token !== "user" ||
    project.enabled ||
    project.enabledSource !== "workspace" ||
    project.optionSources?.token !== "workspace"
  )
    throw new Error("Config scope isolation failed");
  await mutate("plugins/configure", "user", {
    options: { endpoint: "user-edited", count: 3, active: false },
  });
  if (
    (await list("user")).configuredOptions?.endpoint !== "user-edited" ||
    (await read(userFile)).plugins.options?.[pluginId]?.token !== "user-fixture-secret"
  )
    throw new Error("Edited patch failed to preserve secret");
  await mutate("plugins/configure", "workspace", { options: { token: "workspace-new-fixture" } });
  if (
    (await read(workspaceFile)).plugins.options?.[pluginId]?.endpoint !== "workspace-endpoint" ||
    (await list("workspace")).optionSources?.token !== "workspace"
  )
    throw new Error("Secret patch lost unspecified option");
  await mutate("plugins/configure", "user", { options: {}, clearOptionKeys: ["token"] });
  if ((await read(userFile)).plugins.options?.[pluginId]?.token !== undefined)
    throw new Error("Explicit secret clear failed");
  await mutate("plugins/resetConfig", "workspace");
  const resetWorkspace = await read(workspaceFile);
  if (
    resetWorkspace.plugins.enabledPlugins?.[pluginId] !== undefined ||
    resetWorkspace.plugins.options?.[pluginId]?.token !== "workspace-new-fixture" ||
    !(await list("workspace")).enabled
  )
    throw new Error("Workspace reset did not preserve options and inherit enablement");
  await mutate("plugins/resetConfig", "user");
  const resetUser = await read(userFile);
  if (
    resetUser.plugins.enabledPlugins?.[pluginId] !== undefined ||
    resetUser.plugins.options?.[pluginId] !== undefined ||
    (await list("workspace")).configuredOptions?.endpoint !== "workspace-endpoint"
  )
    throw new Error("User reset footprint or workspace preservation failed");
  return {
    pluginConfiguration: true,
    pluginSensitiveProjection: true,
    pluginExplicitClear: true,
    pluginScopedReset: true,
  };
}
