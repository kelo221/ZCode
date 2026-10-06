//! Workspace-scoped plugin queries and mutations over the existing stdio port.

pub use super::plugin_payloads::*;

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub fn fetch_plugins_overview(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.active_ws_key() {
            self.fetch_plugins_overview_for(&key, cx);
        }
    }

    pub(crate) fn fetch_plugins_overview_for(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(ws) = self.ws_mut(key) else { return };
        if !ws.started || ws.inbound.is_none() || ws.inspection.plugins.loading {
            return;
        }
        ws.inspection.plugins.start();
        let mut params = plugins_overview_payload(&ws.path);
        params["workspace"]["workspaceKey"] = json!(ws.key);
        let id = ws.next_id();
        ws.pending.insert(id, Pending::FetchPluginsOverview);
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":"plugins/overview","params":params}).to_string(),
        ) {
            ws.inspection.plugins.fail("Plugin query could not be sent");
        }
        cx.notify();
    }

    pub(crate) fn install_plugin_for(
        &mut self,
        key: &str,
        name: &str,
        market: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = install_plugin_payload(&ws.path, name, market, None, None, None);
        self.send_plugin_operation(key, "plugins/install", params, cx);
    }

    pub(crate) fn uninstall_plugin_for(
        &mut self,
        key: &str,
        id: &str,
        market: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = uninstall_plugin_payload(&ws.path, Some(id), None, Some(market), Some(true));
        self.send_plugin_operation(key, "plugins/uninstall", params, cx);
    }

    pub(crate) fn set_plugin_enabled_for(
        &mut self,
        key: &str,
        id: &str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = set_plugin_enabled_payload(&ws.path, id, enabled, None);
        self.send_plugin_operation(key, "plugins/setEnabled", params, cx);
    }

    pub(crate) fn update_plugin_for(
        &mut self,
        key: &str,
        id: &str,
        market: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = update_plugin_payload(&ws.path, Some(id), Some(market));
        self.send_plugin_operation(key, "plugins/update", params, cx);
    }

    pub(crate) fn restore_builtin_plugin_for(
        &mut self,
        key: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = restore_builtin_plugin_payload(&ws.path, id);
        self.send_plugin_operation(key, "plugins/restoreBuiltin", params, cx);
    }

    pub(crate) fn remove_plugin_marketplace_for(
        &mut self,
        key: &str,
        market: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws(key) else { return };
        let params = remove_marketplace_payload(&ws.path, market);
        self.send_plugin_operation(key, "plugins/marketplace/remove", params, cx);
    }

    pub(crate) fn plugin_operation_available(&self, key: &str) -> bool {
        self.active_ws_key().as_deref() == Some(key)
            && !self.plugin_action_pending(key)
            && !self.plugin_prompt_pending(key)
            && !self.plugin_config_pending(key)
            && self.ws(key).is_some_and(|ws| {
                ws.started
                    && ws.inbound.is_some()
                    && ws
                        .inspection
                        .plugins
                        .value
                        .as_ref()
                        .is_some_and(|v| v.capability_supported)
            })
    }

    pub(crate) fn send_plugin_operation(
        &mut self,
        key: &str,
        method: &str,
        mut params: Value,
        cx: &mut Context<Self>,
    ) {
        // 旧卡片不能按点击时的 active workspace 写入；所有变更复用捕获 owner 的单一路径。
        if !self.plugin_operation_available(key) {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        params["workspace"]["workspaceKey"] = json!(ws.key);
        ws.inspection.plugin_operation_error = None;
        let id = ws.next_id();
        ws.pending.insert(id, Pending::PluginAction(method.into()));
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":method,"params":params}).to_string(),
        ) {
            ws.inspection.plugin_operation_error =
                Some("Plugin operation could not be sent".into());
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "plugin_cmds_tests.rs"]
mod tests;
