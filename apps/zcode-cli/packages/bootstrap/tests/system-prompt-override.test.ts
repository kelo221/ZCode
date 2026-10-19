import { describe, expect, it, vi } from "vitest";
import {
  zcodeWorkspaceReadSystemPromptResultSchema,
  zcodeWorkspaceUpdateSystemPromptResultSchema,
} from "@zcode/shared";
import {
  readSystemPrompt,
  updateSystemPrompt,
} from "../src/zcode-protocol/system-prompt-override.js";
import type { ZCodeProtocolAgentServerContext } from "../src/zcode-protocol/server-types.js";

const workspace = { workspacePath: "/workspace/app", workspaceKey: "/workspace/app" };

function record(parentSessionId?: string) {
  return {
    parentSessionId,
    app: {
      runtime: {
        setSystemPromptOverride: vi.fn(() => true),
        previewDefaultSystemPrompt: vi.fn(() => "default body"),
      },
    },
  };
}

function contextWith(sessions: Array<[string, ReturnType<typeof record>]>) {
  return {
    appRuntimePreferences: {},
    sessions: new Map(sessions),
  } as unknown as ZCodeProtocolAgentServerContext;
}

describe("GPUI system prompt override methods", () => {
  it("caches the override and applies it to resident main sessions only", async () => {
    const main = record();
    const child = record("parent");
    const context = contextWith([
      ["main", main],
      ["child", child],
    ]);

    const result = zcodeWorkspaceUpdateSystemPromptResultSchema.parse(
      await updateSystemPrompt(context, { workspace, prompt: "Be brief." }),
    );

    expect(result).toEqual({ workspace, override: "Be brief.", updatedSessionCount: 1 });
    expect(context.appRuntimePreferences.systemPrompt).toBe("Be brief.");
    expect(main.app.runtime.setSystemPromptOverride).toHaveBeenCalledWith("Be brief.");
    expect(child.app.runtime.setSystemPromptOverride).not.toHaveBeenCalled();
  });

  it("treats null and blank prompts as a reset to the default", async () => {
    const main = record();
    const context = contextWith([["main", main]]);
    await updateSystemPrompt(context, { workspace, prompt: "Be brief." });

    const blank = await updateSystemPrompt(context, { workspace, prompt: "  \n" });
    expect(blank.override).toBeNull();
    expect(context.appRuntimePreferences.systemPrompt).toBeUndefined();
    expect(main.app.runtime.setSystemPromptOverride).toHaveBeenLastCalledWith(undefined);

    const cleared = await updateSystemPrompt(context, { workspace, prompt: null });
    expect(cleared.override).toBeNull();
  });

  it("reads the default from a resident session, or falls back to the static identity", async () => {
    const live = zcodeWorkspaceReadSystemPromptResultSchema.parse(
      await readSystemPrompt(contextWith([["main", record()]]), { workspace }),
    );
    expect(live).toEqual({
      workspace,
      defaultPrompt: "default body",
      defaultSource: "session",
      override: null,
    });

    const empty = zcodeWorkspaceReadSystemPromptResultSchema.parse(
      await readSystemPrompt(contextWith([]), { workspace }),
    );
    expect(empty.defaultSource).toBe("static");
    expect(empty.defaultPrompt).toContain("# Harness");
  });

  it("rejects unknown params", async () => {
    await expect(
      updateSystemPrompt(contextWith([]), { workspace, prompt: "x", extra: true }),
    ).rejects.toThrow();
  });
});
