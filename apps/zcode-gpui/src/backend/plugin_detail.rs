use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::plugin_description::PluginDescription;
use gpui::Context;
use serde_json::{Value, json};

#[derive(Clone, PartialEq)]
pub(crate) struct PluginDetailIdentity {
    pub id: String,
    pub name: String,
    pub marketplace: String,
}
impl PluginDetailIdentity {
    pub(crate) fn cache_key(&self) -> String {
        format!("{}\0{}", self.marketplace, self.name)
    }
}
impl AppState {
    fn plugin_detail_member(&self, key: &str, identity: &PluginDetailIdentity) -> bool {
        self.ws(key)
            .and_then(|w| w.inspection.plugins.value.as_ref())
            .is_some_and(|o| {
                o.capability_supported
                    && o.available_plugins.iter().any(|p| {
                        p.id == identity.id
                            && p.name == identity.name
                            && p.marketplace == identity.marketplace
                    })
            })
    }
    pub(crate) fn open_plugin_detail(
        &mut self,
        key: &str,
        identity: PluginDetailIdentity,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || !self.plugin_detail_member(key, &identity)
            || self.plugin_config_pending(key)
        {
            return;
        }
        let ws = self.ws_mut(key).unwrap();
        ws.inspection.plugin_config = None;
        ws.inspection.plugin_detail = Some(identity.clone());
        self.fetch_plugin_description(key, &identity, false, cx);
        cx.notify();
    }
    pub(crate) fn close_plugin_detail(
        &mut self,
        key: &str,
        identity: &PluginDetailIdentity,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() == Some(key)
            && let Some(w) = self.ws_mut(key)
            && w.inspection.plugin_detail.as_ref() == Some(identity)
        {
            w.inspection.plugin_detail = None;
        }
        cx.notify();
    }
    pub(crate) fn fetch_plugin_description(
        &mut self,
        key: &str,
        identity: &PluginDetailIdentity,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key) || !self.plugin_detail_member(key, identity)
        {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if !ws.started
            || ws.inbound.is_none()
            || ws.inspection.plugin_detail.as_ref() != Some(identity)
        {
            return;
        }
        let cache_key = identity.cache_key();
        if !ws.inspection.plugin_descriptions.contains_key(&cache_key)
            && ws.inspection.plugin_descriptions.len() >= 32
        {
            let removable = ws
                .inspection
                .plugin_descriptions
                .iter()
                .find(|(_, q)| !q.loading)
                .map(|(k, _)| k.clone());
            let Some(removable) = removable else { return };
            ws.inspection.plugin_descriptions.remove(&removable);
        }
        let q = ws
            .inspection
            .plugin_descriptions
            .entry(cache_key.clone())
            .or_default();
        if q.loading || q.attempted && !refresh {
            return;
        }
        q.start();
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::PluginDescribe(identity.clone()));
        if !ws.send_pending_line(id, json!({"id":id,"method":"plugins/describe","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"pluginName":identity.name,"marketplace":identity.marketplace}}).to_string()) {
            ws.inspection.plugin_descriptions.get_mut(&cache_key).unwrap().fail("Plugin description could not be sent");
        }
        cx.notify();
    }
    pub(crate) fn settle_plugin_description(
        &mut self,
        key: &str,
        identity: &PluginDetailIdentity,
        result: Result<Value, String>,
    ) {
        let Some(q) = self.ws_mut(key).and_then(|w| {
            w.inspection
                .plugin_descriptions
                .get_mut(&identity.cache_key())
        }) else {
            return;
        };
        match result.and_then(|v| PluginDescription::parse(&v)) {
            Ok(value) => q.finish(value),
            Err(error) => q.fail(&error),
        }
    }
}
