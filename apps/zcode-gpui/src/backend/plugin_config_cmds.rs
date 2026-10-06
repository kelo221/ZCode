use super::plugin_config::{ConfigAction, ConfigReceipt, ConfigRequest};
use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::plugin_config::{PluginConfigList, validate_mutation};
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn read_plugin_config(&mut self, key: &str, reload: bool, cx: &mut Context<Self>) {
        if self.active_ws_key().as_deref() != Some(key) {
            return;
        }
        self.read_plugin_config_for(key, reload, cx);
    }
    pub(crate) fn read_plugin_config_for(
        &mut self,
        key: &str,
        reload: bool,
        cx: &mut Context<Self>,
    ) {
        if self.plugin_config_pending(key) {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let Some(form) = ws.inspection.plugin_config.as_mut() else {
            return;
        };
        let token = form.token.clone();
        let scope = form.scope;
        form.error = None;
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::PluginConfig(ConfigRequest::Read {
                token: token.clone(),
                reload,
            }),
        );
        if !ws.send_pending_line(id, json!({"id":id,"method":"plugins/list","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"configScope":scope.as_str()}}).to_string()) {
            self.plugin_config_error(key, &token, "Scoped plugin query could not be sent");
        }
        cx.notify();
    }
    pub(crate) fn submit_plugin_config(
        &mut self,
        key: &str,
        token: &str,
        action: ConfigAction,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_config_matches(key, token)
            || self.plugin_config_pending(key)
            || !self.plugin_operation_available(key)
        {
            return;
        }
        let Some(form) = self
            .ws(key)
            .and_then(|w| w.inspection.plugin_config.as_ref())
        else {
            return;
        };
        if form.needs_reload || (action == ConfigAction::Reset && !form.confirmation) {
            return;
        }
        let Some(baseline) = &form.baseline else {
            return;
        };
        if action == ConfigAction::Save {
            let edits = form
                .edits
                .iter()
                .map(|(k, (v, _))| (k.clone(), v.clone()))
                .collect();
            match baseline.patch(&edits) {
                Ok(patch) if !patch.is_empty() => {}
                Ok(_) => return,
                Err(error) => {
                    self.ws_mut(key)
                        .unwrap()
                        .inspection
                        .plugin_config
                        .as_mut()
                        .unwrap()
                        .error = Some(error);
                    cx.notify();
                    return;
                }
            }
        }
        if !self
            .ws(key)
            .and_then(|w| w.inspection.plugins.value.as_ref())
            .is_some_and(|o| o.installed_plugins.iter().any(|p| p.id == form.plugin))
        {
            return;
        }
        let receipt = ConfigReceipt {
            token: token.into(),
            plugin: form.plugin.clone(),
            scope: form.scope,
            baseline: std::sync::Arc::new(baseline.clone()),
            action,
            revision: form.revision,
            versions: form
                .edits
                .iter()
                .map(|(k, (_, v))| (k.clone(), *v))
                .collect(),
        };
        let ws = self.ws_mut(key).unwrap();
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::PluginConfig(ConfigRequest::Preflight(receipt.clone())),
        );
        if !ws.send_pending_line(id, json!({"id":id,"method":"plugins/list","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"configScope":receipt.scope.as_str()}}).to_string()) {
            self.plugin_config_error(key, token, "Plugin configuration preflight could not be sent");
        }
        cx.notify();
    }
    pub(crate) fn settle_plugin_config(
        &mut self,
        key: &str,
        request: ConfigRequest,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        match request {
            ConfigRequest::Read { token, reload } => {
                let parsed = result.and_then(|v| PluginConfigList::parse(&v));
                let Some(form) = self
                    .ws_mut(key)
                    .and_then(|w| w.inspection.plugin_config.as_mut())
                    .filter(|f| f.token == token)
                else {
                    return;
                };
                match parsed.and_then(|list| {
                    list.plugins
                        .into_iter()
                        .find(|p| p.id == form.plugin)
                        .ok_or("Plugin is absent from the scoped list".into())
                }) {
                    Ok(plugin) => {
                        if reload || form.baseline.is_none() {
                            form.baseline = Some(plugin);
                            form.needs_reload = false;
                            form.error = None;
                            form.confirmation = false;
                            form.inputs.clear();
                        }
                    }
                    Err(error) => self.plugin_config_error(key, &token, &error),
                }
            }
            ConfigRequest::Preflight(receipt) => {
                self.settle_plugin_config_preflight(key, receipt, result, cx)
            }
            ConfigRequest::Mutation(receipt) => {
                self.settle_plugin_config_write(key, receipt, result, cx)
            }
        }
        cx.notify();
    }
    fn settle_plugin_config_preflight(
        &mut self,
        key: &str,
        receipt: ConfigReceipt,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let current = result
            .and_then(|v| PluginConfigList::parse(&v))
            .and_then(|list| {
                list.plugins
                    .into_iter()
                    .find(|p| p.id == receipt.plugin)
                    .ok_or("Plugin no longer exists".into())
            });
        if current.as_ref() != Ok(receipt.baseline.as_ref()) {
            // 配置由 CLI 所有；读取发现外部变更时保留输入，禁止继续覆盖或自动重试。
            self.plugin_config_error(
                key,
                &receipt.token,
                &current
                    .err()
                    .unwrap_or("Scoped plugin configuration changed; Reload before writing".into()),
            );
            return;
        }
        if self.plugin_action_pending(key) || self.plugin_prompt_pending(key) {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        let Some(form) = ws
            .inspection
            .plugin_config
            .as_ref()
            .filter(|f| f.token == receipt.token)
        else {
            return;
        };
        if receipt.action == ConfigAction::Reset
            && (form.revision != receipt.revision || !form.confirmation)
        {
            return;
        }
        let mut params = json!({"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"pluginId":receipt.plugin,"scope":receipt.scope.as_str()});
        let method = match receipt.action {
            ConfigAction::Reset => "plugins/resetConfig",
            ConfigAction::Save => {
                let edits = form
                    .edits
                    .iter()
                    .filter(|(k, (_, v))| receipt.versions.get(*k) == Some(v))
                    .map(|(k, (v, _))| (k.clone(), v.clone()))
                    .collect();
                let patch = match receipt.baseline.patch(&edits) {
                    Ok(p) => p,
                    Err(error) => {
                        self.plugin_config_error(key, &receipt.token, &error);
                        return;
                    }
                };
                if patch.is_empty() {
                    return;
                }
                params["options"] = patch.options;
                if !patch.clear_keys.is_empty() {
                    params["clearOptionKeys"] = json!(patch.clear_keys);
                }
                "plugins/configure"
            }
        };
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::PluginConfig(ConfigRequest::Mutation(receipt.clone())),
        );
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":method,"params":params}).to_string(),
        ) {
            self.plugin_config_error(key, &receipt.token, "Plugin write could not be sent");
        }
        cx.notify();
    }
    fn settle_plugin_config_write(
        &mut self,
        key: &str,
        receipt: ConfigReceipt,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = result.and_then(|v| validate_mutation(&v, &receipt.plugin)) {
            // 写入后的错误不能证明未提交；保留草稿并要求 Reload，不重放敏感配置。
            self.plugin_config_error(key, &receipt.token, &error);
            return;
        }
        if let Some(ws) = self.ws_mut(key) {
            ws.inspection.plugin_descriptions.clear();
            ws.pending.retain(|_, p| {
                !matches!(
                    p,
                    Pending::PluginDescribe(_)
                        | Pending::FetchPluginsOverview
                        | Pending::PluginConfig(ConfigRequest::Read { .. })
                )
            });
            ws.inspection.plugins.loading = false;
            if let Some(form) = ws
                .inspection
                .plugin_config
                .as_mut()
                .filter(|f| f.token == receipt.token)
            {
                if receipt.action == ConfigAction::Save {
                    form.edits
                        .retain(|k, (_, v)| receipt.versions.get(k) != Some(v));
                    form.inputs.retain(|k, _| form.edits.contains_key(k));
                }
                form.confirmation = false;
                form.needs_reload = true;
                form.error = None;
            }
        }
        self.fetch_plugins_overview_for(key, cx);
        self.read_plugin_config_for(key, true, cx);
    }
}
