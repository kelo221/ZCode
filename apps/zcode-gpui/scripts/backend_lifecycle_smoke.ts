import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import {
  zcodePluginsMarketplaceMutationResultSchema,
  zcodePluginsOverviewResultSchema,
  zcodePluginsDescribeResultSchema,
  zcodeMcpListResultSchema,
  zcodeWorkflowsListResultSchema,
  zcodeWorkflowsGetResultSchema,
  zcodeWorkflowsRunsResultSchema,
} from "@zcode/shared";
import {
  v4ConversationWorkflowRunArtifactsResultSchema,
  type CommandAck,
  type CommandEnvelope,
} from "@zcode/shared/zcode-protocol-v4";
import { backendWorkflowManagementSmoke } from "./backend_workflow_management_smoke.js";
import { backendPluginConfigSmoke } from "./backend_plugin_config_smoke.js";
import { backendWorkflowArtifactReadSmoke } from "./backend_workflow_artifact_smoke.js";

type Ports = {
  rpc(method: string, params: unknown): Promise<unknown>;
  command(
    type: CommandEnvelope["type"],
    sessionId: string | null,
    payload: unknown,
  ): Promise<CommandAck>;
};

export async function backendLifecycleSmoke(scratch: string, workspace: string, ports: Ports) {
  const { rpc, command } = ports;
  const workspaceRef = { workspacePath: workspace, workspaceKey: workspace };
  const mcpStatus = zcodeMcpListResultSchema.parse(
    await rpc("mcp/list", { workspace: workspaceRef, mode: "status" }),
  );
  if (Object.keys(mcpStatus.statuses).length !== 0)
    throw new Error("Unexpected configured MCP servers in isolated status-only fixture");
  const sourceDir = join(scratch, "personal-source");
  await mkdir(sourceDir);
  const pluginRoot = join(sourceDir, "plugins", "describe-smoke");
  await mkdir(join(pluginRoot, ".zcode-plugin"), { recursive: true });
  await mkdir(join(pluginRoot, "skills", "describe-smoke"), { recursive: true });
  await writeFile(
    join(pluginRoot, ".zcode-plugin", "plugin.json"),
    JSON.stringify({
      name: "describe-smoke",
      version: "0.1.0",
      description: "Disposable local describe fixture",
      skills: "skills",
    }),
  );
  await writeFile(
    join(pluginRoot, "skills", "describe-smoke", "SKILL.md"),
    "---\nname: describe-smoke\ndescription: Disposable describe fixture\n---\n# Fixture\n",
  );
  await writeFile(
    join(sourceDir, "marketplace.json"),
    JSON.stringify({
      name: "gpui-isolated-source",
      owner: { name: "Local Smoke" },
      plugins: [{ name: "describe-smoke", version: "0.1.0", source: "./plugins/describe-smoke" }],
    }),
  );
  const addedSource = zcodePluginsMarketplaceMutationResultSchema.parse(
    await rpc("plugins/marketplace/add", { workspace: workspaceRef, source: sourceDir }),
  );
  const sourceId = addedSource.marketplace?.id;
  if (!sourceId || addedSource.diagnostics?.some((d) => d.severity === "error"))
    throw new Error("Isolated source was not added");
  const refreshedSource = zcodePluginsMarketplaceMutationResultSchema.parse(
    await rpc("plugins/marketplace/update", { workspace: workspaceRef, marketplace: sourceId }),
  );
  if (
    !refreshedSource.marketplaces?.some((m) => m.id === sourceId) ||
    refreshedSource.diagnostics?.some((d) => d.severity === "error")
  )
    throw new Error("Isolated source did not refresh");
  const sourcesOverview = zcodePluginsOverviewResultSchema.parse(
    await rpc("plugins/overview", { workspace: workspaceRef }),
  );
  if (!sourcesOverview.marketplaces.some((m) => m.id === sourceId))
    throw new Error("Added source is absent from overview");
  const described = zcodePluginsDescribeResultSchema.parse(
    await rpc("plugins/describe", {
      workspace: workspaceRef,
      pluginName: "describe-smoke",
      marketplace: sourceId,
    }),
  );
  if (
    described.diagnostics?.some((d) => d.severity === "error") ||
    !described.components.some(
      (c) => c.kind === "skill" && c.items.some((i) => i.name === "describe-smoke"),
    )
  )
    throw new Error("Disposable plugin description was not returned");
  zcodePluginsMarketplaceMutationResultSchema.parse(
    await rpc("plugins/marketplace/remove", { workspace: workspaceRef, marketplace: sourceId }),
  );
  const removedOverview = zcodePluginsOverviewResultSchema.parse(
    await rpc("plugins/overview", { workspace: workspaceRef }),
  );
  if (removedOverview.marketplaces.some((m) => m.id === sourceId))
    throw new Error("Removed isolated source is still present");
  const savedDir = join(workspace, ".zcode", "workflows");
  await mkdir(savedDir, { recursive: true });
  await writeFile(
    join(savedDir, "isolated.dwf.ts"),
    '/* zcode-workflow\ndescription: Isolated smoke workflow\n*/\nawait artifact.markdown("smoke-report", "# Isolated report", { title: "Smoke report", primary: true });\nreturn { ok: true };\n',
  );
  const savedList = zcodeWorkflowsListResultSchema.parse(
    await rpc("workflows/list", { workspace: workspaceRef, scope: "project" }),
  );
  if (!savedList.workflows.some((w) => w.name === "isolated"))
    throw new Error("Isolated saved workflow is absent");
  const definition = zcodeWorkflowsGetResultSchema.parse(
    await rpc("workflows/get", { workspace: workspaceRef, scope: "project", name: "isolated" }),
  );
  if (!definition.ok || definition.name !== "isolated" || !definition.script.includes("return"))
    throw new Error("Isolated workflow definition could not be inspected");
  const workflowTarget = await command("createSession", null, { workspaceId: workspace });
  if (workflowTarget.status !== "accepted" || workflowTarget.result?.type !== "createSession")
    throw new Error("Workflow target session creation failed");
  const workflowSid = workflowTarget.result.sessionId;
  const launchedWorkflow = await command("startSavedWorkflow", workflowSid, {
    name: "isolated",
    scope: "project",
  });
  let savedWorkflowLaunch: "accepted" | "capabilityUnsupported";
  let workflowMarkdownContent = false;
  let workflowArtifactParentAuthorization = false;
  if (
    launchedWorkflow.status === "accepted" &&
    launchedWorkflow.result?.type === "startSavedWorkflow"
  ) {
    savedWorkflowLaunch = "accepted";
    const runId = launchedWorkflow.result.runId;
    let completed = false;
    for (let attempt = 0; attempt < 50; attempt++) {
      const history = zcodeWorkflowsRunsResultSchema.parse(
        await rpc("workflows/runs", {
          workspace: workspaceRef,
          scope: "project",
          name: "isolated",
          limit: 50,
        }),
      );
      const run = history.runs.find((r) => r.runId === runId);
      if (run?.status === "completed") {
        if (run.parentSessionId !== workflowSid || !run.toolCallId)
          throw new Error("Saved workflow history lost its parent/tool identity");
        completed = true;
        break;
      }
      if (run && (run.status === "errored" || run.status === "stopped"))
        throw new Error(`Isolated workflow settled as ${run.status}`);
      await sleep(100);
    }
    if (!completed) throw new Error("Isolated workflow completion was not present in history");
    const artifacts = v4ConversationWorkflowRunArtifactsResultSchema.parse(
      await rpc("v4/conversation/workflowRunArtifacts", { sessionId: workflowSid, runId }),
    );
    const deliverable = artifacts.artifacts.find((a) => a.id === "smoke-report");
    if (
      deliverable?.kind !== "markdown" ||
      deliverable.title !== "Smoke report" ||
      deliverable.version !== 1 ||
      deliverable.primary !== true ||
      deliverable.versions.length !== 1 ||
      deliverable.itemCount !== 0
    )
      throw new Error("Published workflow artifact metadata was not returned");
    const unknownRun = v4ConversationWorkflowRunArtifactsResultSchema.parse(
      await rpc("v4/conversation/workflowRunArtifacts", {
        sessionId: workflowSid,
        runId: "unknown-isolated-run",
      }),
    );
    if (unknownRun.artifacts.length !== 0) throw new Error("Unknown run fabricated artifacts");
    const otherTarget = await command("createSession", null, { workspaceId: workspace });
    if (otherTarget.status !== "accepted" || otherTarget.result?.type !== "createSession")
      throw new Error("Artifact authorization fixture could not create a target");
    const content = await backendWorkflowArtifactReadSmoke(
      workflowSid,
      otherTarget.result.sessionId,
      runId,
      rpc,
    );
    workflowMarkdownContent = content.workflowMarkdownContent;
    workflowArtifactParentAuthorization = content.workflowArtifactParentAuthorization;
    const cleanup = await command("deleteSession", otherTarget.result.sessionId, {});
    if (cleanup.status !== "accepted" && cleanup.status !== "noop")
      throw new Error("Artifact authorization fixture cleanup failed");
  } else if (
    launchedWorkflow.status === "failed" &&
    launchedWorkflow.reasonCode === "fault.command.capabilityUnsupported"
  ) {
    savedWorkflowLaunch = "capabilityUnsupported";
    const cleanup = await command("deleteSession", workflowSid, {});
    if (cleanup.status !== "accepted" && cleanup.status !== "noop")
      throw new Error("Unsupported workflow target cleanup failed");
  } else {
    throw new Error(`Unexpected saved workflow launch: ${JSON.stringify(launchedWorkflow)}`);
  }
  return {
    ...(await backendWorkflowManagementSmoke(scratch, workspace, rpc)),
    ...(await backendPluginConfigSmoke(scratch, workspace, rpc)),
    mcpStatusInspection: true,
    personalSourceLifecycle: true,
    pluginDescription: true,
    savedWorkflowLaunch,
    savedWorkflowDefinition: true,
    savedWorkflowCompletedInHistory: savedWorkflowLaunch === "accepted",
    workflowArtifactMetadata: savedWorkflowLaunch === "accepted",
    workflowMarkdownContent,
    workflowArtifactParentAuthorization,
    workflowUnknownRunEmptyArtifacts: savedWorkflowLaunch === "accepted",
  };
}
