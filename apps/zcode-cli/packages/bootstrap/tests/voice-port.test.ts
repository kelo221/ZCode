import { describe, expect, it, vi } from "vitest";
import { zcodeProtocolMethods } from "@zcode/shared";
import { createProtocolVoicePort } from "../src/zcode-protocol/voice-port.js";
import type { ZCodeProtocolAgentServerContext } from "../src/zcode-protocol/server-types.js";
import { ZCodeProtocolAgentServer } from "../src/zcode-protocol/server.js";
import { closeSession, createSessionRecordForV4 } from "../src/zcode-protocol/server-operations.js";
import type { ZCodeAppOptions } from "../src/app/types.js";
import { createFakeApp } from "./helpers/fake-zcode-app.js";

it.each([
  { voiceEnabled: true, expected: true },
  { voiceEnabled: false, expected: false },
  { voiceEnabled: undefined, expected: false },
])(
  "injects the voice port only when the host enables voice: %j",
  async ({ voiceEnabled, expected }) => {
    let appOptions: ZCodeAppOptions | undefined;
    const server = new ZCodeProtocolAgentServer({
      createZCodeApp: (options) => {
        appOptions = options;
        return createFakeApp(options);
      },
    });
    const context = (server as unknown as { context: ZCodeProtocolAgentServerContext }).context;
    const requestClient = vi.spyOn(context, "requestClient").mockImplementation(async (method) => {
      if (method === zcodeProtocolMethods.sessionRequestRuntimePreferences) {
        return {
          nativeSearchEnhancementsEnabled: true,
          memoryEnabled: false,
          askUserQuestionAutoResolutionEnabled: true,
          modelContextBudgetStrategy: "preflight-v1",
          ...(voiceEnabled === undefined ? {} : { voiceEnabled }),
        };
      }
      return { accepted: true };
    });
    const result = await createSessionRecordForV4(context, {
      workspace: { workspacePath: "/fixture", workspaceKey: "/fixture" },
    });
    try {
      expect(Boolean(appOptions?.voicePort)).toBe(expected);
      if (expected) {
        await expect(
          appOptions!.voicePort!.speak({ message: "Plan ready", kind: "planReady" }),
        ).resolves.toEqual({ accepted: true });
        expect(requestClient).toHaveBeenCalledWith(
          zcodeProtocolMethods.interactionSpeak,
          { sessionId: result.sessionId, message: "Plan ready", kind: "planReady" },
          expect.anything(),
          expect.objectContaining({ timeoutMs: expect.any(Number) }),
        );
      }
    } finally {
      await closeSession(context, { sessionId: result.sessionId });
    }
  },
);

function contextWith(requestClient: ReturnType<typeof vi.fn>): ZCodeProtocolAgentServerContext {
  return { requestClient } as unknown as ZCodeProtocolAgentServerContext;
}

describe("createProtocolVoicePort", () => {
  it("passes through the client's rejection reason", async () => {
    const port = createProtocolVoicePort(
      contextWith(vi.fn(async () => ({ accepted: false, reason: "disabled" }))),
      "sess_1",
    );
    await expect(port.speak({ message: "Done", kind: "done" })).resolves.toEqual({
      accepted: false,
      reason: "disabled",
    });
  });

  it("maps a rejection without a reason to unavailable", async () => {
    const port = createProtocolVoicePort(
      contextWith(vi.fn(async () => ({ accepted: false }))),
      "sess_1",
    );
    await expect(port.speak({ message: "Done", kind: "done" })).resolves.toEqual({
      accepted: false,
      reason: "unavailable",
    });
  });

  it("maps method-not-found, no client and timeouts to unavailable", async () => {
    const port = createProtocolVoicePort(
      contextWith(
        vi.fn(async () => {
          throw new Error("Unsupported ZCode Protocol request: interaction/speak");
        }),
      ),
      "sess_1",
    );
    await expect(port.speak({ message: "Done", kind: "done" })).resolves.toEqual({
      accepted: false,
      reason: "unavailable",
    });
  });

  it("rethrows when the tool call was cancelled", async () => {
    const controller = new AbortController();
    controller.abort();
    const port = createProtocolVoicePort(
      contextWith(
        vi.fn(async () => {
          throw new Error("cancelled");
        }),
      ),
      "sess_1",
    );
    await expect(
      port.speak({ message: "Done", kind: "done" }, { signal: controller.signal }),
    ).rejects.toThrow("cancelled");
  });
});
