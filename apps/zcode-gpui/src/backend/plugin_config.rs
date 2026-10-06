use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::plugin_config::{ConfigEdit, ConfigPlugin, ConfigScope};
use gpui::Context;
use std::collections::BTreeMap;

pub(crate) struct PluginConfigDraft {
    pub token: String,
    pub plugin: String,
    pub scope: ConfigScope,
    pub baseline: Option<ConfigPlugin>,
    pub edits: BTreeMap<String, (ConfigEdit, u64)>,
    pub inputs: BTreeMap<String, gpui::Entity<ely_gpui_component::forms::TextInput>>,
    pub revision: u64,
    pub visible: bool,
    pub needs_reload: bool,
    pub confirmation: bool,
    pub error: Option<String>,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ConfigAction {
    Save,
    Reset,
}
#[derive(Clone)]
pub(crate) struct ConfigReceipt {
    pub token: String,
    pub plugin: String,
    pub scope: ConfigScope,
    pub baseline: std::sync::Arc<ConfigPlugin>,
    pub action: ConfigAction,
    pub revision: u64,
    pub versions: BTreeMap<String, u64>,
}
pub(crate) enum ConfigRequest {
    Read { token: String, reload: bool },
    Preflight(ConfigReceipt),
    Mutation(ConfigReceipt),
}
impl AppState {
    pub(crate) fn plugin_config_feedback<'a>(
        &self,
        key: &str,
        id: u64,
        message: &'a str,
    ) -> &'a str {
        // 配置错误可能回显无标签密钥；该边界使用通用反馈，不依赖正则脱敏。
        if self
            .ws(key)
            .is_some_and(|w| matches!(w.pending.get(&id), Some(Pending::PluginConfig(_))))
        {
            "Plugin configuration request failed; Reload before writing"
        } else {
            message
        }
    }
    pub(crate) fn plugin_config_pending(&self, key: &str) -> bool {
        self.ws(key).is_some_and(|w| {
            w.pending
                .values()
                .any(|p| matches!(p, Pending::PluginConfig(_)))
        })
    }
    pub(crate) fn plugin_config_matches(&self, key: &str, token: &str) -> bool {
        self.active_ws_key().as_deref() == Some(key)
            && self.viewing_child.is_none()
            && self
                .ws(key)
                .and_then(|w| w.inspection.plugin_config.as_ref())
                .is_some_and(|f| f.visible && f.token == token)
    }
    pub(crate) fn open_plugin_config(
        &mut self,
        key: &str,
        plugin: &str,
        scope: ConfigScope,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_operation_available(key)
            || self.viewing_child.is_some()
            || self.plugin_config_pending(key)
        {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if !ws
            .inspection
            .plugins
            .value
            .as_ref()
            .is_some_and(|o| o.installed_plugins.iter().any(|p| p.id == plugin))
        {
            return;
        }
        ws.inspection.plugin_detail = None;
        ws.inspection.plugin_config = Some(PluginConfigDraft {
            token: uuid::Uuid::now_v7().to_string(),
            plugin: plugin.into(),
            scope,
            baseline: None,
            edits: BTreeMap::new(),
            inputs: BTreeMap::new(),
            revision: 0,
            visible: true,
            needs_reload: false,
            confirmation: false,
            error: None,
        });
        self.read_plugin_config(key, false, cx);
    }
    pub(crate) fn close_plugin_config(&mut self, key: &str, token: &str, cx: &mut Context<Self>) {
        if !self.plugin_config_matches(key, token) {
            return;
        }
        let pending = self.plugin_config_pending(key);
        if let Some(ws) = self.ws_mut(key) {
            if pending {
                ws.inspection.plugin_config.as_mut().unwrap().visible = false;
            } else {
                ws.inspection.plugin_config = None;
            }
        }
        cx.notify();
    }
    pub(crate) fn edit_plugin_config(
        &mut self,
        key: &str,
        token: &str,
        option: &str,
        edit: ConfigEdit,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_config_matches(key, token) {
            return;
        }
        let Some(form) = self
            .ws_mut(key)
            .and_then(|w| w.inspection.plugin_config.as_mut())
        else {
            return;
        };
        if !form
            .baseline
            .as_ref()
            .is_some_and(|b| b.declaration(option).is_some())
        {
            return;
        }
        form.revision = form
            .revision
            .checked_add(1)
            .expect("plugin form revision exhausted");
        if edit == ConfigEdit::Clear {
            form.inputs.remove(option);
        }
        form.edits.insert(option.into(), (edit, form.revision));
        form.confirmation = false;
        cx.notify();
    }
    pub(crate) fn confirm_plugin_reset(
        &mut self,
        key: &str,
        token: &str,
        confirm: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_config_matches(key, token) || self.plugin_config_pending(key) {
            return;
        }
        if let Some(form) = self
            .ws_mut(key)
            .and_then(|w| w.inspection.plugin_config.as_mut())
        {
            form.confirmation = confirm;
        }
        cx.notify();
    }
    pub(crate) fn plugin_config_error(&mut self, key: &str, token: &str, error: &str) {
        if let Some(form) = self
            .ws_mut(key)
            .and_then(|w| w.inspection.plugin_config.as_mut())
            .filter(|f| f.token == token)
        {
            form.error = Some(crate::shared::redact::scrub(error));
            form.needs_reload = true;
            form.confirmation = false;
        }
    }
}
