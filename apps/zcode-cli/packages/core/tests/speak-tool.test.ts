import { describe, expect, it, vi } from "vitest";
import type { TraceId, VoicePort } from "@zcode/contracts";
import { createToolRegistry, registerBuiltInTools } from "../src/tool/index.js";
import { PermissionService } from "../src/permission/service.js";
import { resolveRuntimePermissionCapability } from "../src/tool/executor/permission-capability.js";
import { speakToolEntry } from "../src/tool/handlers/speak.js";
import type { ToolExecutionContext } from "../src/tool/types.js";

function createContext(port?: VoicePort): ToolExecutionContext {
  return {
    toolCallId: "tool-speak",
    traceId: "trace-speak" as TraceId,
    abortSignal: new AbortController().signal,
    voicePort: port,
    workingDirectory: "/workspace",
    workspaceRoot: "/workspace",
    sessionId: "sess-speak" as never,
  };
}

describe("Speak tool", () => {
  it("is registered only with includeSpeak", () => {
    const without = createToolRegistry();
    registerBuiltInTools(without, {});
    expect(without.has("Speak")).toBe(false);

    const withFlag = createToolRegistry();
    registerBuiltInTools(withFlag, { includeSpeak: true });
    expect(withFlag.has("Speak")).toBe(true);
  });

  it("exposes provider-compatible JSON schemas", () => {
    for (const schema of [speakToolEntry.inputSchema, speakToolEntry.outputSchema]) {
      const text = JSON.stringify(schema);
      for (const key of ["anyOf", "$ref", "$defs", "definitions"]) {
        expect(text).not.toContain(`"${key}"`);
      }
    }
    expect(speakToolEntry.inputSchema).toMatchObject({
      type: "object",
      required: ["message", "kind"],
      additionalProperties: false,
    });
  });

  it("needs no approval and tells the model to keep it short and rare", () => {
    expect(speakToolEntry.metadata.needsApproval).toBe(false);
    expect(speakToolEntry.permission.needsApproval).toBe(false);
    const instructions = speakToolEntry.metadata.modelInstructions?.join("\n") ?? "";
    expect(instructions).toContain("one plain spoken sentence");
    expect(instructions).toContain("Do not narrate progress");
  });

  it("runs without a permission prompt in every mode", () => {
    const capability = resolveRuntimePermissionCapability(
      speakToolEntry,
      { message: "Task done", kind: "done" },
      { workingDirectory: "/workspace", workspaceRoot: "/workspace" },
    );
    const service = new PermissionService();
    for (const mode of ["build", "edit", "plan", "yolo", "guarded"] as const) {
      const decision = service.checkPermission(
        {
          input: { message: "Task done", kind: "done" },
          mode,
          riskLevel: "low",
          toolName: "Speak",
        },
        capability,
      );
      expect(decision, mode).toMatchObject({ allowed: true, decision: "allow" });
    }
  });

  it("forwards the message and kind and reports success", async () => {
    const speak = vi.fn(async () => ({ accepted: true as const }));
    const output = await speakToolEntry.handler(
      { message: "  Plan ready for review ", kind: "planReady" },
      createContext({ speak }),
    );
    expect(speak).toHaveBeenCalledWith(
      { message: "Plan ready for review", kind: "planReady" },
      expect.objectContaining({ signal: expect.any(AbortSignal) }),
    );
    expect(output).toEqual({ spoken: true });
    expect(speakToolEntry.formatModelContent?.(output)).toBe("Spoken aloud to the user.");
  });

  it("reports rejections with a reason the model can act on", async () => {
    const output = await speakToolEntry.handler(
      { message: "Done", kind: "done" },
      createContext({ speak: async () => ({ accepted: false, reason: "disabled" }) }),
    );
    expect(output).toEqual({ spoken: false, reason: "disabled" });
    expect(speakToolEntry.formatModelContent?.(output)).toContain("Don't call Speak again");
  });

  it("fails cleanly without a voice port", async () => {
    const output = await speakToolEntry.handler(
      { message: "Done", kind: "done" },
      createContext(undefined),
    );
    expect(output).toMatchObject({
      result: false,
      message: expect.stringContaining("voice_unavailable"),
    });
  });

  it("rejects empty, overlong or unknown-kind input", async () => {
    const ctx = createContext({ speak: async () => ({ accepted: true }) });
    await expect(speakToolEntry.handler({ message: "  ", kind: "done" }, ctx)).rejects.toThrow();
    await expect(
      speakToolEntry.handler({ message: "a".repeat(301), kind: "done" }, ctx),
    ).rejects.toThrow();
    await expect(speakToolEntry.handler({ message: "hi", kind: "yell" }, ctx)).rejects.toThrow();
  });
});
