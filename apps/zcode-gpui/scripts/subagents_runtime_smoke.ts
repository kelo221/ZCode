/**
 * Standalone scratch-only acceptance; run with Bun and an existing
 * ZCODE_GPUI_SERVICES_RUNTIME (ZCode.exe with ELECTRON_RUN_AS_NODE=1).
 * Host owns profile writes. One CLI app-server owns both runtime snapshots.
 * Evidence is the model-facing Agent name/description schema, not child execution.
 * Order: Host v1 -> held A -> Host v2 -> B -> correlated next user turn on A.
 */
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ServiceChannels,
  type AgentSummary,
  type AgentsListResult,
  type SubAgentConfig,
} from "@zcode/shared";
import { v4ConversationSubscribeResultSchema } from "@zcode/shared/zcode-protocol-v4";
import { backendMock, smokeProviderConfig } from "./backend_mock.js";
import {
  alive,
  assert,
  bounded,
  children,
  connectCli,
  connectHost,
  fail,
  launch,
  record,
  stop,
  waitFor,
} from "./subagents_runtime_transport.js";

const repo = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
const v1 = "Synthetic runtime schema revision one",
  v2 = "Synthetic runtime schema revision two";
const turnMarkers = {
  aFirst: "ZCODE_SCHEMA_A_FIRST",
  b: "ZCODE_SCHEMA_B",
  aNext: "ZCODE_SCHEMA_A_NEXT",
};
let stage = "runtime preflight",
  reason: string | undefined,
  scratch: string | undefined;
let mock: Awaited<ReturnType<typeof backendMock>> | undefined;
let hostConnection: Awaited<ReturnType<typeof connectHost>> | undefined;
const checks: Record<string, boolean> = {};
function agentDescription(body: unknown): string | undefined {
  const tools = record(body)?.tools;
  if (!Array.isArray(tools)) return undefined;
  const agents = tools
    .map((tool) => record(record(tool)?.function))
    .filter((tool) => tool?.name === "Agent");
  if (agents.length === 0) return undefined;
  assert(agents.length === 1, "Expected one model-facing Agent tool");
  const description = agents[0]?.description;
  assert(typeof description === "string", "Missing Agent tool description");
  return description;
}
function hasUserMarker(body: unknown, marker: string) {
  const messages = record(body)?.messages;
  return (
    Array.isArray(messages) &&
    messages.some((message) => {
      const m = record(message);
      if (m?.role !== "user") return false;
      return typeof m.content === "string"
        ? m.content.includes(marker)
        : Array.isArray(m.content) &&
            m.content.some((part) => {
              const p = record(part);
              return p?.type === "text" && typeof p.text === "string" && p.text.includes(marker);
            });
    })
  );
}
function verifySchema(body: unknown, expected: string, forbidden: string) {
  const description = agentDescription(body);
  assert(description, "Missing model-facing Agent tool");
  assert(description.includes(`- scratch-profile: ${expected}`), "Expected profile schema absent");
  assert(!description.includes(forbidden), "Unexpected profile revision leaked into schema");
}
const deadline = setTimeout(() => {
  fail("Standalone acceptance deadline");
  for (const child of children) if (alive(child)) child.kill();
}, 90000);
try {
  assert(process.versions.bun, "Run this standalone script with installed Bun");
  const runtime = process.env.ZCODE_GPUI_SERVICES_RUNTIME;
  assert(runtime, "Provide an installed Services runtime; no installation is performed");
  scratch = await mkdtemp(join(tmpdir(), "gpui-subagents-runtime-"));
  const home = join(scratch, "home"),
    data = join(scratch, "data");
  const workspace = join(scratch, "workspace"),
    temp = join(scratch, "temp");
  const storage = join(home, ".zcode");
  const providerFile = join(data, ".zcode", "v2", "provider_config.json");
  for (const path of [home, workspace, temp, storage, join(data, ".zcode", "v2")])
    await mkdir(path, { recursive: true });
  mock = await backendMock();
  let modelRequests = 0;
  mock.server.on("request", (_request, response) => {
    if (++modelRequests > 8) {
      fail("Model request count bound");
      response.destroy();
    }
  });
  const modelSelection = {
    providerId: "smoke-openai",
    modelId: "mock-chat",
    options: { reasoningLevel: "low" },
  };
  // Services resolves this default file independently of CLI's explicit personal override.
  await writeFile(providerFile, JSON.stringify(smokeProviderConfig(modelSelection, mock.port)));
  const env: NodeJS.ProcessEnv = {};
  for (const key of ["PATH", "SystemRoot", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"])
    if (process.env[key]) env[key] = process.env[key];
  Object.assign(env, {
    HOME: home,
    USERPROFILE: home,
    ZCODE_DESKTOP_HOME_DIR: home,
    ZCODE_DATA_BASE_DIR: data,
    APPDATA: join(home, "AppData", "Roaming"),
    LOCALAPPDATA: join(home, "AppData", "Local"),
    TEMP: temp,
    TMP: temp,
    ZCODE_STORAGE_DIR: storage,
    ZCODE_SESSION_DB_PATH: join(data, "session.db"),
    ZCODE_RUNTIME_ENV: "desktop",
    ZCODE_SERVICE_AUTHORITY_MODE: "desktop-attached-remote",
    ZCODE_BUILTIN_PROVIDER_CONFIG_FILE: join(repo, "config", "provider", "zcode-builtin.json"),
    ZCODE_PERSONAL_PROVIDER_CONFIG_FILE: providerFile,
  });
  stage = "Services Host handshake";
  const host = launch(
    runtime,
    ["--no-warnings", join(repo, "packages/server/dist/remote/zcode-server.cjs")],
    { ...env, ELECTRON_RUN_AS_NODE: "1" },
    workspace,
  );
  hostConnection = await connectHost(host);
  const hostCall = hostConnection.call;
  stage = "shared provider fixture";
  const view = await hostCall<{
    providers: { providerId: string; models: { modelId: string }[] }[];
  }>(ServiceChannels.ModelSelection, "getView", []);
  assert(
    view.providers.some(
      (p) => p.providerId === "smoke-openai" && p.models.some((m) => m.modelId === "mock-chat"),
    ),
    "Host did not read canonical synthetic provider fixture",
  );
  checks.sharedCanonicalProviderFixture = true;
  const config = (description: string): SubAgentConfig => ({
    name: "scratch-profile",
    description,
    systemPrompt: "Disposable synthetic profile; no real user data.",
    tools: ["Read"],
  });
  stage = "Host persists profile v1";
  const created = await hostCall<{ agent: AgentSummary }>(
    ServiceChannels.Subagents,
    "createAgent",
    [{ provider: "glm", scope: "user", config: config(v1) }],
  );
  const profileFile = join(storage, "agents", "scratch-profile.md");
  assert(
    resolve(created.agent.path) === resolve(profileFile),
    "Host profile escaped canonical scratch root",
  );
  assert((await readFile(profileFile, "utf8")).includes(v1), "Profile v1 did not persist");
  checks.hostRpcPersistedV1 = true;
  stage = "single CLI app-server startup";
  const cli = launch(
    process.execPath,
    [
      join(repo, "apps/zcode-cli/packages/cli/src/main.ts"),
      "app-server",
      "--stdio",
      "--surface",
      "desktop",
    ],
    env,
    workspace,
  );
  const { command, rpc, phases, preferences, waitReady } = connectCli(cli);
  async function session() {
    const ack = await command("createSession", null, {
      workspaceId: workspace,
      config: { modelSelection, mode: "yolo" },
    });
    assert(
      ack.status === "accepted" && ack.result?.type === "createSession",
      "Session creation rejected",
    );
    const sid = ack.result.sessionId;
    const sub = v4ConversationSubscribeResultSchema.parse(
      await rpc("v4/conversation/subscribe", {
        topic: `conversation/${sid}`,
        connectionId: `smoke-${sid}`,
        clientMode: "desktop-continuous",
        workspace: { workspacePath: workspace, workspaceKey: workspace },
        visibility: "foreground",
      }),
    );
    return { sid, subscriptionId: sub.ack.subscriptionId };
  }
  async function send(s: Awaited<ReturnType<typeof session>>, marker: string) {
    phases.delete(s.subscriptionId);
    const ack = await command("sendText", s.sid, {
      text: marker,
      requestedDelivery: "startNow",
      modelSelection,
    });
    assert(
      ack.status === "accepted" &&
        ack.result?.type === "inputAccepted" &&
        ack.result.delivery === "startNow",
      "User turn was not admitted startNow",
    );
  }
  await waitReady();
  const a = await session();
  stage = "A observes v1 while active";
  const hold = mock.holdNext();
  await send(a, turnMarkers.aFirst);
  await bounded(hold.arrived, "A held model request");
  assert(mock.requests() === 1, "Unexpected model request before Host update");
  const aRequest = mock
    .requestBodies()
    .find((body) => hasUserMarker(body, turnMarkers.aFirst) && agentDescription(body));
  verifySchema(aRequest, v1, v2);
  await waitFor(() => phases.get(a.subscriptionId) === "running", "A running public phase");
  checks.aHeldActiveSawV1 = true;
  stage = "Host updates v2 while A stays active";
  const updated = await hostCall<{ agent: AgentSummary }>(
    ServiceChannels.Subagents,
    "updateAgent",
    [
      {
        agentId: created.agent.id,
        oldFilePath: created.agent.path,
        provider: "glm",
        scope: "user",
        config: config(v2),
      },
    ],
  );
  assert(
    updated.agent.id === created.agent.id && resolve(updated.agent.path) === resolve(profileFile),
    "Profile identity changed",
  );
  const saved = await readFile(profileFile, "utf8");
  assert(saved.includes(v2) && !saved.includes(v1), "Profile v2 did not replace v1");
  const listed = await hostCall<AgentsListResult>(ServiceChannels.Subagents, "list", [
    { workspacePath: workspace, mode: "settingsUserOnly" },
  ]);
  assert(
    listed.agents.some((p) => p.id === created.agent.id && p.description === v2),
    "Host readback mismatch",
  );
  assert(
    alive(cli) && phases.get(a.subscriptionId) === "running" && mock.requests() === 1,
    "Host update disturbed active A",
  );
  checks.hostRpcPersistedV2WhileARunning = true;
  stage = "B materializes v2 on same CLI server";
  const b = await session();
  assert(a.sid !== b.sid, "Expected distinct session IDs");
  await send(b, turnMarkers.b);
  const bRequests = () =>
    mock!
      .requestBodies()
      .filter((body) => hasUserMarker(body, turnMarkers.b) && agentDescription(body));
  await waitFor(() => bRequests().length > 0, "Correlated B model request");
  for (const body of bRequests()) verifySchema(body, v2, v1);
  await waitFor(() => phases.get(b.subscriptionId) === "completedSuccess", "B public completion");
  assert(phases.get(a.subscriptionId) === "running", "A did not stay running while B completed");
  checks.sameServerBSawV2 = true;
  stage = "A next public turn remains v1";
  hold.release();
  await waitFor(() => phases.get(a.subscriptionId) === "completedSuccess", "A public completion");
  const nextHold = mock.holdNext();
  await send(a, turnMarkers.aNext);
  await bounded(nextHold.arrived, "Next public request hold");
  nextHold.release();
  // 后续轮次与后台请求可能交错；仅用合成 user 标记关联，不能把请求序号或完成事件当成轮次。
  const nextSchemas = () =>
    mock!
      .requestBodies()
      .filter((body) => hasUserMarker(body, turnMarkers.aNext) && agentDescription(body));
  await waitFor(() => nextSchemas().length > 0, "Correlated A next Agent schema");
  assert(
    preferences.has("runtime-materialization") && preferences.has("user-execution"),
    "Runtime preference handshake missing",
  );
  checks.runtimePreferenceScopesObserved = true;
  assert(children.length === 2 && alive(cli) && alive(host), "Owned server lifecycle changed");
  checks.oneHostOneCliServer = true;
  const observed = nextSchemas();
  checks.aNextTurnAdvertisedV2 = observed.some((body) =>
    agentDescription(body)?.includes(`- scratch-profile: ${v2}`),
  );
  checks.aNextTurnSchemaImmutable = observed.every((body) => {
    const d = agentDescription(body)!;
    return d.includes(`- scratch-profile: ${v1}`) && !d.includes(v2);
  });
  assert(checks.aNextTurnSchemaImmutable, "A next-turn Agent schema did not retain v1");
} catch (error) {
  // Only harness-generated categories are published; raw RPC errors and requests are never printed.
  reason =
    error instanceof Error
      ? error.message.replace(/https?:\/\/\S+/g, "[url]")
      : "Acceptance failed";
  process.exitCode = 1;
} finally {
  clearTimeout(deadline);
  mock?.releaseAll();
  hostConnection?.dispose();
  try {
    await Promise.all(children.map(stop));
    if (mock) {
      mock.server.closeAllConnections();
      await bounded(
        new Promise<void>((ok) => mock!.server.close(() => ok())),
        "Mock cleanup",
        3000,
      );
    }
    if (scratch)
      await bounded(
        rm(scratch, { recursive: true, force: true, maxRetries: 2, retryDelay: 100 }),
        "Scratch cleanup",
        5000,
      );
    checks.ownedProcessesAndScratchCleaned = true;
  } catch {
    stage = "resource cleanup";
    process.exitCode = 1;
  }
  const schemaEvidence = mock?.requestBodies().map((body) => {
    const d = agentDescription(body);
    return {
      agentTool: Boolean(d),
      v1: Boolean(d?.includes(v1)),
      v2: Boolean(d?.includes(v2)),
      aNext: hasUserMarker(body, turnMarkers.aNext),
      b: hasUserMarker(body, turnMarkers.b),
    };
  });
  console.log(
    JSON.stringify({
      result: process.exitCode ? "failed" : "passed",
      ...(process.exitCode ? { stage, reason } : {}),
      assertions: checks,
      mockRequests: mock?.requests() ?? 0,
      schemaEvidence,
      evidence: "Agent tool schema name/description only",
      limits: [
        "No Agent child execution or profile systemPrompt/model/reasoning effects asserted",
        "A next-turn completion not asserted",
        "No native GUI or remote replay acceptance",
      ],
    }),
  );
}
