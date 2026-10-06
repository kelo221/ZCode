import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { zcodeWorkspacePresentationSchema, zcodeSessionStateSnapshotSchema } from "@zcode/shared";

export async function prepareSlashCatalogSmoke(workspace: string) {
  const commands = join(workspace, ".zcode", "commands");
  await mkdir(commands, { recursive: true });
  await writeFile(
    join(commands, "zcode_probe.md"),
    "---\ndescription: Scratch catalog probe\nargument-hint: <value>\n---\nScratch probe body.\n",
  );
  await writeFile(
    join(commands, "zcode_disabled_probe.md"),
    "---\ndescription: Disabled scratch probe\ndisable-noninteractive: true\n---\nDisabled probe body.\n",
  );
}

export async function backendSlashCatalogSmoke(
  workspace: string,
  sessionId: string,
  rpc: (method: string, params: unknown) => Promise<unknown>,
  command: (type: "sendText", sessionId: string, payload: unknown) => Promise<{ status: string }>,
  requestBodies: () => unknown[],
  waitFor: (predicate: () => boolean, label: string) => Promise<void>,
) {
  const presentation = zcodeWorkspacePresentationSchema.parse(
    await rpc("workspace/readPresentation", {
      workspace: { workspacePath: workspace, workspaceKey: workspace },
    }),
  );
  const snapshot = zcodeSessionStateSnapshotSchema.parse(
    await rpc("session/read", { sessionId, messageLimit: 1 }),
  );
  if (
    presentation.workspace.workspaceKey !== workspace ||
    snapshot.session.sessionId !== sessionId ||
    snapshot.session.workspace.workspaceKey !== workspace
  )
    throw new Error("Slash catalog owner mismatch");
  for (const catalog of [presentation.slashCommands, snapshot.slashCommands]) {
    if (!catalog) throw new Error("Missing session slash catalog");
    if (!catalog.some((entry) => entry.name === "init" && entry.source === "builtin"))
      throw new Error("Missing CLI-owned init command");
    if (
      !catalog.some(
        (entry) =>
          entry.name === "zcode_probe" &&
          entry.source === "custom" &&
          entry.inputHint === "/zcode_probe <value>",
      )
    )
      throw new Error("Missing scoped custom command");
    if (catalog.some((entry) => entry.name === "zcode_disabled_probe"))
      throw new Error("Disabled custom command leaked into catalog");
  }
  let unknownSessionRejected = false;
  try {
    await rpc("session/read", { sessionId: "missing-session", messageLimit: 1 });
  } catch {
    unknownSessionRejected = true;
  }
  if (!unknownSessionRejected) throw new Error("Unknown session slash read did not fail closed");
  const start = requestBodies().length;
  const initialized = await command("sendText", sessionId, {
    text: "/init scratch-only-init-notes",
    requestedDelivery: "startNow",
  });
  if (initialized.status !== "accepted") throw new Error("Init command was not admitted");
  await waitFor(
    () =>
      requestBodies()
        .slice(start)
        .some((body) => {
          const text = JSON.stringify(body);
          return (
            text.includes("You are running ZCode's built-in /init command.") &&
            text.includes("scratch-only-init-notes") &&
            text.includes("AGENTS.md")
          );
        }),
    "CLI-owned init expansion",
  );
  return { slashCatalogs: true, disabledSlashCommandFiltered: true, initPromptExpansion: true };
}
