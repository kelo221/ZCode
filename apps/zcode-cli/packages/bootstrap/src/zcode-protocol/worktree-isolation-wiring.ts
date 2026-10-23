import type { CodeSearchPort, DiagnosticsPort, WorktreeIsolationPort } from "@zcode/contracts";
import { createAutoWorktreeIsolation } from "@zcode/adapters";
import type { ZCodeWorkspaceRef } from "@zcode/shared";
import type { ZCodeProtocolAgentServerContext } from "./server-types.js";

/**
 * Agent `isolation: "worktree"` 的会话端口（Fork，见 07-subagent.md「Worktree isolation」）。
 * 只给本地工作区注入；按根绑定的服务为每个 worktree 另取一份：code search 用新的索引键
 * （不复用父 checkout 的索引），忙闲跟随父 workspace；诊断带上 worktree 的 CARGO_TARGET_DIR。
 * 必须在 code search / 诊断端口解析之后调用，这样只在父会话已启用的服务上派生。
 */
export async function resolveSessionWorktreeIsolation(
  context: ZCodeProtocolAgentServerContext,
  workspace: ZCodeWorkspaceRef,
  input: { remote: boolean; codeSearchEnabled: boolean; lspDiagnosticsEnabled: boolean },
): Promise<{
  worktreeIsolationPort?: WorktreeIsolationPort;
  scopePortsFor?: (scope: { root: string; env?: Record<string, string> }) => {
    codeSearchPort?: CodeSearchPort;
    diagnosticsPort?: DiagnosticsPort;
  };
}> {
  if (input.remote) return {};
  const codeSearch =
    input.codeSearchEnabled && context.codeSearch && (await context.codeSearch.available())
      ? context.codeSearch
      : undefined;
  const diagnostics = input.lspDiagnosticsEnabled ? context.diagnostics : undefined;
  return {
    // jj 仓库（且 jj 可执行）用 jj workspace，其余用 git worktree；见 07-subagent.md「jj 后端」。
    worktreeIsolationPort: createAutoWorktreeIsolation(),
    scopePortsFor: ({ root, env }) => ({
      ...(codeSearch
        ? {
            codeSearchPort: codeSearch.portFor({
              workspaceKey: `${workspace.workspaceKey}#worktree:${root}`,
              root,
              busyKey: workspace.workspaceKey,
            }),
          }
        : {}),
      ...(diagnostics
        ? {
            diagnosticsPort: diagnostics.portFor({
              workspaceKey: workspace.workspaceKey,
              ...(env ? { env } : {}),
            }),
          }
        : {}),
    }),
  };
}
