import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import {
  zcodeWorkflowsGetResultSchema,
  zcodeWorkflowsUpdateMetaResultSchema,
  zcodeWorkflowsDeleteResultSchema,
  zcodeWorkflowsMoveResultSchema,
  zcodeWorkflowsListResultSchema,
} from "@zcode/shared";

type Rpc = (method: string, params: unknown) => Promise<unknown>;

export async function backendWorkflowManagementSmoke(scratch: string, workspace: string, rpc: Rpc) {
  const workspaceRef = { workspacePath: workspace, workspaceKey: workspace };
  const name = "management-fixture";
  const globalDir = join(scratch, "home", ".zcode", "workflows");
  await mkdir(globalDir, { recursive: true });
  await writeFile(
    join(globalDir, `${name}.dwf.ts`),
    "/* zcode-workflow\ndescription: Disposable management fixture\nwhenToUse: Before edit\n*/\nreturn { fixture: true };\n",
    { flag: "wx" },
  );
  const original = zcodeWorkflowsGetResultSchema.parse(
    await rpc("workflows/get", { workspace: workspaceRef, name, scope: "global" }),
  );
  if (!original.ok) throw new Error("Disposable global definition was not inspected");
  const updated = zcodeWorkflowsUpdateMetaResultSchema.parse(
    await rpc("workflows/updateMeta", {
      workspace: workspaceRef,
      name,
      scope: "global",
      meta: { description: "Edited fixture", args: { j: { type: "json", default: null } } },
    }),
  );
  if (!updated.ok || updated.path !== original.path)
    throw new Error("Fixture metadata update failed");
  const edited = zcodeWorkflowsGetResultSchema.parse(
    await rpc("workflows/get", { workspace: workspaceRef, name, scope: "global" }),
  );
  if (
    !edited.ok ||
    edited.script !== original.script ||
    edited.meta.description !== "Edited fixture" ||
    edited.meta.whenToUse !== undefined ||
    edited.meta.args?.j?.default !== null
  )
    throw new Error("Metadata replacement lost script, reset semantics or default:null");
  const moved = zcodeWorkflowsMoveResultSchema.parse(
    await rpc("workflows/move", { workspace: workspaceRef, name }),
  );
  if (!moved.ok || moved.from !== original.path || !moved.to)
    throw new Error("Fixture move failed");
  const project = zcodeWorkflowsGetResultSchema.parse(
    await rpc("workflows/get", { workspace: workspaceRef, name, scope: "project" }),
  );
  if (!project.ok || project.path !== moved.to || project.script !== original.script)
    throw new Error("Moved fixture was not inspected in the destination");
  const global = zcodeWorkflowsGetResultSchema.parse(
    await rpc("workflows/get", { workspace: workspaceRef, name, scope: "global" }),
  );
  if (global.ok || global.reason !== "not_found") throw new Error("Move left the original fixture");
  await writeFile(
    join(globalDir, `${name}.dwf.ts`),
    "/* zcode-workflow\ndescription: Refusal fixture\n*/\nreturn 2;\n",
    { flag: "wx" },
  );
  const refused = zcodeWorkflowsMoveResultSchema.parse(
    await rpc("workflows/move", { workspace: workspaceRef, name }),
  );
  if (refused.ok || refused.reason !== "target_exists")
    throw new Error("Move overwrote existing destination");
  for (const scope of ["project", "global"] as const) {
    const removed = zcodeWorkflowsDeleteResultSchema.parse(
      await rpc("workflows/delete", { workspace: workspaceRef, name, scope }),
    );
    if (!removed.ok) throw new Error("Disposable fixture delete failed");
    const list = zcodeWorkflowsListResultSchema.parse(
      await rpc("workflows/list", { workspace: workspaceRef, scope }),
    );
    if (list.workflows.some((w) => w.name === name))
      throw new Error("Deleted fixture remained in list");
  }
  return {
    savedWorkflowMetadata: true,
    savedWorkflowMove: true,
    savedWorkflowMoveRefusal: true,
    savedWorkflowDelete: true,
  };
}
