import type { CodeSearchPort } from "@zcode/contracts";
import { loadFffCodeSearchEngine } from "@zcode/adapters";
import { CodeSearchManager } from "./code-search-manager.js";
import type { ZCodeProtocolAgentServerContext } from "./server-types.js";
import type { ZCodeWorkspaceRef } from "@zcode/shared";

/**
 * 会话的 FindFiles/SearchCode 端口。只有 Host 下发 codeSearchEnabled（GPUI）且 fff 原生库
 * 可加载时才返回；Electron 不发这个偏好，因此永远不会 import fff。
 */
export async function resolveSessionCodeSearchPort(
  context: ZCodeProtocolAgentServerContext,
  workspace: ZCodeWorkspaceRef,
  enabled: boolean,
): Promise<{ codeSearchPort?: CodeSearchPort }> {
  if (!enabled) return {};
  context.codeSearch ??= new CodeSearchManager({
    load: loadFffCodeSearchEngine,
    isWorkspaceBusy: (workspaceKey) => isWorkspaceBusy(context, workspaceKey),
    logger: context.logger,
  });
  if (!(await context.codeSearch.available())) return {};
  return {
    codeSearchPort: context.codeSearch.portFor({
      workspaceKey: workspace.workspaceKey,
      root: workspace.workspacePath,
    }),
  };
}

/**
 * 与 session 常驻池（session-residency.ts readResidencyFacts）同一组「忙」事实，
 * 但不计订阅者：打开着却空闲的对话不应让索引常驻。
 */
export function isWorkspaceBusy(
  context: ZCodeProtocolAgentServerContext,
  workspaceKey: string,
): boolean {
  for (const [sessionId, record] of context.sessions) {
    if (record.workspace.workspaceKey !== workspaceKey) continue;
    if (
      record.activeAbortController !== undefined ||
      (record.residencyFinalizationCount ?? 0) > 0 ||
      record.app.runtime.hasResidencyBlockingWork() ||
      context.v4Interactions.hasPendingForSession(sessionId) ||
      (context.v4Gateway?.hasResidencyBlockingCommands(sessionId) ?? false)
    ) {
      return true;
    }
  }
  return false;
}
