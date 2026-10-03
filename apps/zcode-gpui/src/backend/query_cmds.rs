//! Read-only queries for workspace statistics and MCP status:
//! `v4/usage/stats` and `mcp/list`.

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::json;

impl AppState {
    /// Fetch usage stats for the active workspace (`v4/usage/stats`).
    pub fn fetch_usage_stats(&mut self, range: &str, cx: &mut Context<Self>) {
        let Some(ws_key) = self.active_ws_key() else {
            return;
        };
        let Some(ws) = self.ws_mut(&ws_key) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::FetchUsageStats(range.to_string()));
        ws.send_line(
            json!({
                "id": id,
                "method": "v4/usage/stats",
                "params": {
                    "range": range,
                }
            })
            .to_string(),
        );
        cx.notify();
    }

    /// Query connected MCP servers for the active workspace (`mcp/list`).
    pub fn fetch_mcp_servers(&mut self, cx: &mut Context<Self>) {
        let Some(ws_key) = self.active_ws_key() else {
            return;
        };
        let Some(ws) = self.ws_mut(&ws_key) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let id = ws.next_id();
        let ws_path = std::path::PathBuf::from(&ws.key);
        ws.pending.insert(id, Pending::FetchMcpList);
        ws.send_line(
            json!({
                "id": id,
                "method": "mcp/list",
                "params": {
                    "mode": "status",
                    "workspace": crate::backend::plugin_payloads::workspace_ref(&ws_path)
                }
            })
            .to_string(),
        );
        cx.notify();
    }
}
