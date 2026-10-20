import type { DiagnosticsPort } from "@zcode/contracts";
import type { ZCodeWorkspaceRef } from "@zcode/shared";
import { isWorkspaceBusy } from "./code-search-wiring.js";
import { DiagnosticsManager } from "./diagnostics-manager.js";
import type { ZCodeProtocolAgentServerContext } from "./server-types.js";

/**
 * 会话的语言服务器诊断端口。只有 Host 下发 lspDiagnosticsEnabled（GPUI）时才返回；
 * Electron 不发这个偏好，因此不会启动任何语言服务器。
 */
export function resolveSessionDiagnosticsPort(
  context: ZCodeProtocolAgentServerContext,
  workspace: ZCodeWorkspaceRef,
  enabled: boolean,
): { diagnosticsPort?: DiagnosticsPort } {
  if (!enabled) return {};
  context.diagnostics ??= new DiagnosticsManager({
    isWorkspaceBusy: (workspaceKey) => isWorkspaceBusy(context, workspaceKey),
    logger: context.logger,
  });
  return { diagnosticsPort: context.diagnostics.portFor({ workspaceKey: workspace.workspaceKey }) };
}
