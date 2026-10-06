//! Workspace-scoped inspection queries; results are owned by each connection handle.
use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::json;

impl AppState {
    pub fn fetch_usage_stats(&mut self, range: &str, cx: &mut Context<Self>) {
        if !matches!(range, "7d" | "30d" | "all") {
            return;
        }
        let Some(key) = self.active_ws_key() else {
            return;
        };
        let Some(ws) = self.ws_mut(&key) else {
            return;
        };
        ws.inspection.usage_range = range.into();
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let query = ws.inspection.usage.entry(range.into()).or_default();
        if query.loading {
            return;
        }
        query.start();
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::FetchUsageStats(range.into()));
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":"v4/usage/stats","params":{"range":range}}).to_string(),
        ) {
            ws.inspection
                .usage
                .get_mut(range)
                .unwrap()
                .fail("Usage query could not be sent");
        }
        cx.notify();
    }

    pub fn fetch_mcp_servers(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.active_ws_key() else {
            return;
        };
        let Some(ws) = self.ws_mut(&key) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() || ws.inspection.mcp.loading {
            return;
        }
        ws.inspection.mcp.start();
        let id = ws.next_id();
        ws.pending.insert(id, Pending::FetchMcpList);
        if !ws.send_pending_line(id, json!({"id":id,"method":"mcp/list","params":{"mode":"status","workspace":crate::backend::plugin_payloads::workspace_ref(&ws.path)}}).to_string()) {
            ws.inspection.mcp.fail("MCP query could not be sent");
        }
        cx.notify();
    }
}
