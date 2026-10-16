import type { VoicePort } from "@zcode/contracts";
import {
  ZCODE_SPEAK_REQUEST_TIMEOUT_MS,
  zcodeProtocolMethods,
  zcodeSpeakResultSchema,
} from "@zcode/shared";
import type { ZCodeProtocolAgentServerContext } from "./server-types.js";

/**
 * Speak 工具的协议端口：转成 `interaction/speak` 反向请求，由 GPUI 桌面读出。
 * 只在 runtime preferences 下发 voiceEnabled 时注入（见 resolveSessionStartupPreferences）。
 * 客户端缺方法（-32601）、无客户端（-32020）、超时或任何传输错误都归为 unavailable：
 * 语音只是提醒，失败不应让模型的回合报错。
 */
export function createProtocolVoicePort(
  context: ZCodeProtocolAgentServerContext,
  sessionId: string,
): VoicePort {
  return {
    async speak(request, options) {
      try {
        const result = await context.requestClient(
          zcodeProtocolMethods.interactionSpeak,
          { sessionId, message: request.message, kind: request.kind },
          zcodeSpeakResultSchema,
          {
            timeoutMs: ZCODE_SPEAK_REQUEST_TIMEOUT_MS,
            ...(options?.signal ? { signal: options.signal } : {}),
          },
        );
        if (result.accepted) return { accepted: true };
        return { accepted: false, reason: result.reason ?? "unavailable" };
      } catch (error) {
        if (options?.signal?.aborted) throw error;
        context.logger?.debug("Speak request fell back to unavailable", {
          errorMessage: error instanceof Error ? error.message : String(error),
          event: "zcode_protocol.voice.speak_unavailable",
          module: "bootstrap.zcode_protocol",
          sessionId,
        });
        return { accepted: false, reason: "unavailable" };
      }
    },
  };
}
