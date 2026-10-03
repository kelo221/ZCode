//! Outbound commands and queries for the plugin store:
//! `plugins/{overview,install,uninstall,update,setEnabled,restoreBuiltin,marketplace/*}`.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts and CONTEXT.md.

pub use super::plugin_payloads::*;

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::json;

impl AppState {
    /// Fetch plugin store overview (`plugins/overview`).
    pub fn fetch_plugins_overview(&mut self, cx: &mut Context<Self>) {
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
        ws.pending.insert(id, Pending::FetchPluginsOverview);
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/overview",
                "params": plugins_overview_payload(&ws_path)
            })
            .to_string(),
        );
        cx.notify();
    }

    /// Install an available plugin (`plugins/install`).
    pub fn install_plugin(&mut self, plugin_name: &str, marketplace: &str, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction(format!("install {plugin_name} from {marketplace}")),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/install",
                "params": install_plugin_payload(&ws_path, plugin_name, marketplace, None, None, None)
            })
            .to_string(),
        );
        self.push_log(format!("installing plugin {plugin_name}…"));
        cx.notify();
    }

    /// Uninstall an installed plugin (`plugins/uninstall`).
    pub fn uninstall_plugin(&mut self, plugin_id: &str, marketplace: &str, cx: &mut Context<Self>) {
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
        ws.pending
            .insert(id, Pending::PluginAction(format!("uninstall {plugin_id}")));
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/uninstall",
                "params": uninstall_plugin_payload(&ws_path, Some(plugin_id), None, Some(marketplace), Some(true))
            })
            .to_string(),
        );
        self.push_log(format!("uninstalling plugin {plugin_id}…"));
        cx.notify();
    }

    /// Toggle plugin enable/disable state (`plugins/setEnabled`).
    pub fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction(format!("set {plugin_id} enabled={enabled}")),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/setEnabled",
                "params": set_plugin_enabled_payload(&ws_path, plugin_id, enabled, None)
            })
            .to_string(),
        );
        self.push_log(format!("setting plugin {plugin_id} enabled={enabled}…"));
        cx.notify();
    }

    /// Update an installed plugin to latest version (`plugins/update`).
    pub fn update_plugin(&mut self, plugin_id: &str, marketplace: &str, cx: &mut Context<Self>) {
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
        ws.pending
            .insert(id, Pending::PluginAction(format!("update {plugin_id}")));
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/update",
                "params": update_plugin_payload(&ws_path, Some(plugin_id), Some(marketplace))
            })
            .to_string(),
        );
        self.push_log(format!("updating plugin {plugin_id}…"));
        cx.notify();
    }

    /// Restore a restorable builtin plugin (`plugins/restoreBuiltin`).
    pub fn restore_builtin_plugin(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction(format!("restore builtin {plugin_id}")),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/restoreBuiltin",
                "params": restore_builtin_plugin_payload(&ws_path, plugin_id)
            })
            .to_string(),
        );
        self.push_log(format!("restoring builtin plugin {plugin_id}…"));
        cx.notify();
    }

    /// Add a personal source marketplace (`plugins/marketplace/add`).
    #[allow(dead_code)]
    pub fn add_plugin_marketplace(&mut self, source: &str, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction(format!("add marketplace {source}")),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/marketplace/add",
                "params": add_marketplace_payload(&ws_path, source, None, None)
            })
            .to_string(),
        );
        self.push_log(format!("adding marketplace {source}…"));
        cx.notify();
    }

    /// Remove a personal source marketplace (`plugins/marketplace/remove`).
    pub fn remove_plugin_marketplace(&mut self, marketplace: &str, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction(format!("remove marketplace {marketplace}")),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/marketplace/remove",
                "params": remove_marketplace_payload(&ws_path, marketplace)
            })
            .to_string(),
        );
        self.push_log(format!("removing marketplace {marketplace}…"));
        cx.notify();
    }

    /// Refresh/update marketplaces (`plugins/marketplace/update`).
    #[allow(dead_code)]
    pub fn update_plugin_marketplace(&mut self, marketplace: Option<&str>, cx: &mut Context<Self>) {
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
        ws.pending.insert(
            id,
            Pending::PluginAction("refresh marketplaces".to_string()),
        );
        ws.send_line(
            json!({
                "id": id,
                "method": "plugins/marketplace/update",
                "params": update_marketplace_payload(&ws_path, marketplace, None)
            })
            .to_string(),
        );
        self.push_log("refreshing marketplaces…".to_string());
        cx.notify();
    }
}

#[cfg(test)]
#[path = "plugin_cmds_tests.rs"]
mod tests;
