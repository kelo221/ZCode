import { spawn } from "node:child_process";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { randomUUID } from "node:crypto";
import { backendMock, smokeProviderConfig } from "./backend_mock.js";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import {
  commandEnvelopeSchema,
  commandAckSchema,
  conversationTopicWireFrameSchema,
  v4ConversationSubscribeResultSchema,
  v4ConversationResyncResultSchema,
  type CommandEnvelope,
} from "@zcode/shared/zcode-protocol-v4";
import {
  zcodeSkillsReferenceCatalogResultSchema,
  zcodePluginsReferenceCatalogResultSchema,
  zcodeSessionSubagentsResultSchema,
  zcodeSessionRequestRuntimePreferencesParamsSchema,
  zcodeSessionRuntimePreferencesResultSchema,
} from "@zcode/shared";
import { backendLifecycleSmoke } from "./backend_lifecycle_smoke.js";
import { backendDeliverySmoke } from "./backend_delivery_smoke.js";
import { backendAttachmentSmoke } from "./backend_attachment_smoke.js";
import { backendSlashCatalogSmoke, prepareSlashCatalogSmoke } from "./backend_slash_smoke.js";
import { preparePluginConfigSmoke } from "./backend_plugin_config_smoke.js";

const root = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const scratch = await mkdtemp(join(tmpdir(), "zcode-gpui-smoke-"));
const mock = await backendMock();
const server = mock.server;
const modelSelection = {
  providerId: "smoke-openai",
  modelId: "mock-chat",
  options: { reasoningLevel: "low" },
};
const config = smokeProviderConfig(modelSelection, mock.port);
const home = join(scratch, "home");
const workspace = join(scratch, "workspace");
const providerFile = join(scratch, "provider_config.json");
await mkdir(home, { recursive: true });
await mkdir(workspace, { recursive: true });
await writeFile(providerFile, JSON.stringify(config));
await preparePluginConfigSmoke(scratch, workspace);
await prepareSlashCatalogSmoke(workspace);
const env: NodeJS.ProcessEnv = {
  PATH: process.env.PATH,
  SystemRoot: process.env.SystemRoot,
  WINDIR: process.env.WINDIR,
  TEMP: scratch,
  TMP: scratch,
  HOME: home,
  USERPROFILE: home,
  APPDATA: home,
  LOCALAPPDATA: home,
  ZCODE_RUNTIME_ENV: "desktop",
  ZCODE_DESKTOP_HOME_DIR: home,
  ZCODE_DATA_BASE_DIR: join(scratch, "data"),
  ZCODE_STORAGE_DIR: join(scratch, "storage"),
  ZCODE_SESSION_DB_PATH: join(scratch, "session.db"),
  ZCODE_BUILTIN_PROVIDER_CONFIG_FILE: join(root, "config/provider/zcode-builtin.json"),
  ZCODE_PERSONAL_PROVIDER_CONFIG_FILE: providerFile,
};
const child = spawn(
  process.execPath,
  [
    join(root, "apps/zcode-cli/packages/cli/src/main.ts"),
    "app-server",
    "--stdio",
    "--surface",
    "desktop",
  ],
  { cwd: workspace, env, stdio: ["pipe", "pipe", "pipe"] },
);
type Message = {
  id?: number | string;
  method?: string;
  params?: { subscriptionId?: string; phase?: string };
  error?: unknown;
  result?: unknown;
};
const messages: Message[] = [];
const runtimePreferenceScopes = new Set<string>();
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();
let stderr = "";
child.stderr.on("data", (chunk) => {
  stderr = (stderr + chunk.toString()).slice(-8000);
});
child.on("error", (error) => {
  for (const request of pending.values()) request.reject(error);
});
const lines = createInterface({ input: child.stdout });
lines.on("line", (line) => {
  let message: Message;
  try {
    message = JSON.parse(line);
  } catch {
    return;
  }
  if (message.method && message.id !== undefined) {
    if (message.method === "session/requestRuntimePreferences") {
      const params = zcodeSessionRequestRuntimePreferencesParamsSchema.parse(message.params);
      runtimePreferenceScopes.add(params.scope);
      child.stdin.write(
        JSON.stringify({
          id: message.id,
          result: zcodeSessionRuntimePreferencesResultSchema.parse({
            askUserQuestionAutoResolutionEnabled: true,
            nativeSearchEnhancementsEnabled: true,
            memoryEnabled: true,
          }),
        }) + "\n",
      );
    } else
      child.stdin.write(
        JSON.stringify({
          id: message.id,
          error: { code: -32603, message: "Not available in isolated smoke" },
        }) + "\n",
      );
    return;
  }
  if (typeof message.id === "number") {
    const request = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) request?.reject(new Error(JSON.stringify(message.error)));
    else request?.resolve(message.result);
  } else messages.push(message);
});
let nextId = 0;
async function waitFor(predicate: () => boolean, label: string, timeout = 30000) {
  const end = Date.now() + timeout;
  while (!predicate()) {
    if (child.exitCode !== null)
      throw new Error(`${label}: backend exited ${child.exitCode}; ${stderr}`);
    if (Date.now() > end) throw new Error(`${label}: timed out; ${stderr}`);
    await sleep(20);
  }
}
function rpc(method: string, params: unknown): Promise<unknown> {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method}: timed out; ${stderr}`));
    }, 30000);
    pending.set(id, {
      resolve(value) {
        clearTimeout(timer);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timer);
        reject(error);
      },
    });
    child.stdin.write(JSON.stringify({ id, method, params }) + "\n");
  });
}
async function command(type: CommandEnvelope["type"], sessionId: string | null, payload: unknown) {
  const params = commandEnvelopeSchema.parse({
    commandId: randomUUID(),
    clientId: "gpui-isolated-smoke",
    sessionId,
    type,
    issuedAt: Date.now(),
    payload,
  });
  return commandAckSchema.parse(await rpc("v4/command", params));
}
function frames(subscriptionId: string) {
  return messages
    .filter(
      (message) =>
        message.method === "v4/conversation/frame" &&
        message.params?.subscriptionId === subscriptionId,
    )
    .map((message) => {
      const wire = conversationTopicWireFrameSchema.parse(message.params);
      if (wire.kind !== "complete") throw new Error("Unexpected fragmented smoke frame");
      return wire.frame;
    });
}
function hasReply(subscriptionId: string) {
  return frames(subscriptionId).some((frame) =>
    JSON.stringify(frame.payload).includes("ZCODE_SMOKE_OK"),
  );
}
try {
  await waitFor(
    () => messages.some((m) => m.method === "startup/storageState" && m.params?.phase === "ready"),
    "storage startup",
  );
  const created = await command("createSession", null, {
    workspaceId: workspace,
    config: { modelSelection, mode: "yolo" },
  });
  if (
    created.status !== "accepted" ||
    created.result?.type !== "createSession" ||
    !created.result.sessionId
  )
    throw new Error(`create: ${JSON.stringify(created)}`);
  const sid = created.result.sessionId;
  const workspaceRef = { workspacePath: workspace, workspaceKey: workspace };
  const draftSkills = zcodeSkillsReferenceCatalogResultSchema.parse(
    await rpc("skills/referenceCatalog", { workspace: workspaceRef }),
  );
  const draftPlugins = zcodePluginsReferenceCatalogResultSchema.parse(
    await rpc("plugins/referenceCatalog", { workspace: workspaceRef }),
  );
  if (draftSkills.authority !== "workspace" || draftPlugins.authority !== "workspace")
    throw new Error("Draft catalog authority mismatch");
  const subscribe = async (
    mode: "desktop-continuous" | "web-remote-replayable",
    connectionId: string,
  ) =>
    v4ConversationSubscribeResultSchema.parse(
      await rpc("v4/conversation/subscribe", {
        topic: `conversation/${sid}`,
        connectionId,
        clientMode: mode,
        workspace: { workspacePath: workspace, workspaceKey: workspace },
        visibility: "foreground",
      }),
    );
  const live = await subscribe("desktop-continuous", "smoke-live");
  const replay = await subscribe("web-remote-replayable", "smoke-replay");
  const sent = await command("sendText", sid, {
    text: "Reply with exactly the test marker",
    requestedDelivery: "startNow",
    modelSelection,
  });
  if (
    sent.status !== "accepted" ||
    sent.result?.type !== "inputAccepted" ||
    sent.result.delivery !== "startNow" ||
    sent.result.inputId !== sent.commandId
  )
    throw new Error(`One-shot startNow admission mismatch: ${JSON.stringify(sent)}`);
  await waitFor(
    () => hasReply(live.ack.subscriptionId) && hasReply(replay.ack.subscriptionId),
    "assistant frames",
  );
  const recoveryStart = messages.length;
  const resync = v4ConversationResyncResultSchema.parse(
    await rpc("v4/conversation/resync", {
      topic: `conversation/${sid}`,
      subscriptionId: live.ack.subscriptionId,
      connectionId: "smoke-live",
      base: null,
      forceSnapshot: true,
    }),
  );
  await waitFor(
    () =>
      messages.slice(recoveryStart).some((message) => {
        if (
          message.method !== "v4/conversation/frame" ||
          message.params?.subscriptionId !== resync.ack.subscriptionId
        )
          return false;
        const wire = conversationTopicWireFrameSchema.parse(message.params);
        return (
          wire.kind === "complete" &&
          wire.deliveryKind === "recovery" &&
          wire.frame.payload.kind === "snapshot" &&
          wire.frame.payload.snapshot.control.phase === "completedSuccess" &&
          JSON.stringify(wire.frame.payload.snapshot.rows).includes("ZCODE_SMOKE_OK")
        );
      }),
    "completed recovery snapshot",
  );
  const sessionSkills = zcodeSkillsReferenceCatalogResultSchema.parse(
    await rpc("skills/referenceCatalog", { workspace: workspaceRef, sessionId: sid }),
  );
  const sessionPlugins = zcodePluginsReferenceCatalogResultSchema.parse(
    await rpc("plugins/referenceCatalog", { workspace: workspaceRef, sessionId: sid }),
  );
  if (sessionSkills.authority !== "session" || sessionPlugins.authority !== "session")
    throw new Error("Session catalog authority mismatch");
  let unknownSessionRejected = false;
  try {
    await rpc("skills/referenceCatalog", { workspace: workspaceRef, sessionId: "missing-session" });
  } catch {
    unknownSessionRejected = true;
  }
  if (!unknownSessionRejected) throw new Error("Unknown session catalog did not fail closed");
  const directory = zcodeSessionSubagentsResultSchema.parse(
    await rpc("session/subagents", { sessionId: sid, endedLimit: 20 }),
  );
  if (directory.ended.total !== 0 || directory.ended.items.length !== 0)
    throw new Error("Unexpected subagent directory for isolated parent");
  const slashCatalogs = await backendSlashCatalogSmoke(
    workspace,
    sid,
    rpc,
    command,
    mock.requestBodies,
    waitFor,
  );
  const attachments = await backendAttachmentSmoke(sid, {
    rpc,
    command,
    requestBodies: mock.requestBodies,
    waitFor,
  });
  const delivery = await backendDeliverySmoke(workspace, modelSelection, {
    rpc,
    messages,
    holdNext: mock.holdNext,
  });
  const lifecycle = await backendLifecycleSmoke(scratch, workspace, { rpc, command });
  if (mock.requests() === 0) throw new Error("No mock model request occurred");
  if (
    !runtimePreferenceScopes.has("runtime-materialization") ||
    !runtimePreferenceScopes.has("user-execution")
  )
    throw new Error("Runtime preferences were not requested for both scopes");
  console.log(
    JSON.stringify({
      result: "passed",
      mockRequests: mock.requests(),
      liveFrames: frames(live.ack.subscriptionId).length,
      replayFrames: frames(replay.ack.subscriptionId).length,
      recovery: true,
      isolatedData: true,
      referenceCatalogs: true,
      subagentDirectory: true,
      runtimePreferences: true,
      oneShotStartNowAdmission: true,
      ...slashCatalogs,
      ...attachments,
      ...delivery,
      ...lifecycle,
      unknownSessionRejected,
    }),
  );
} finally {
  child.stdin.end();
  if (child.exitCode === null) {
    child.kill();
    await Promise.race([
      new Promise<void>((resolve) => child.once("exit", () => resolve())),
      sleep(3000),
    ]);
  }
  lines.close();
  mock.releaseAll();
  server.closeAllConnections();
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await rm(scratch, { recursive: true, force: true });
}
