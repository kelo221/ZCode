import { buildIdentitySection } from "@zcode/core";
import {
  zcodeWorkspaceReadSystemPromptParamsSchema,
  zcodeWorkspaceUpdateSystemPromptParamsSchema,
} from "@zcode/shared";
import { parseParams, type ZCodeProtocolAgentServerContext } from "./server-types.js";

/**
 * GPUI 专属的主 Agent system prompt 覆盖（Electron 从不调用这两个方法）。
 *
 * CLI 进程按 workspace 隔离：覆盖缓存在 appRuntimePreferences 供后续 create/resume 继承，
 * 同时立即下发给已驻留的主会话。覆盖不落盘；GPUI 在每次 agent 就绪时重新同步。
 */
export async function updateSystemPrompt(
  context: ZCodeProtocolAgentServerContext,
  rawParams: unknown,
) {
  const params = parseParams(zcodeWorkspaceUpdateSystemPromptParamsSchema, rawParams);
  const override = params.prompt?.trim() ? params.prompt : undefined;
  context.appRuntimePreferences.systemPrompt = override;

  let updatedSessionCount = 0;
  for (const record of context.sessions.values()) {
    if (record.parentSessionId) continue;
    if (record.app.runtime.setSystemPromptOverride?.(override)) updatedSessionCount += 1;
  }

  return { workspace: params.workspace, override: override ?? null, updatedSessionCount };
}

export async function readSystemPrompt(
  context: ZCodeProtocolAgentServerContext,
  rawParams: unknown,
) {
  const params = parseParams(zcodeWorkspaceReadSystemPromptParamsSchema, rawParams);
  const override = context.appRuntimePreferences.systemPrompt ?? null;
  const preview = previewFromResidentSession(context);
  return {
    workspace: params.workspace,
    defaultPrompt: (preview ?? buildIdentitySection().content).trim(),
    defaultSource: preview === undefined ? ("static" as const) : ("session" as const),
    override,
  };
}

function previewFromResidentSession(context: ZCodeProtocolAgentServerContext): string | undefined {
  for (const record of context.sessions.values()) {
    if (record.parentSessionId) continue;
    const preview = record.app.runtime.previewDefaultSystemPrompt?.();
    if (preview) return preview;
  }
  return undefined;
}
