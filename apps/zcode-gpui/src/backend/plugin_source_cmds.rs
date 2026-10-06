use crate::app::store::AppState;
use crate::backend::plugin_payloads::{add_marketplace_payload, update_marketplace_payload};
use gpui::Context;
use serde_json::Value;

impl AppState {
    pub(crate) fn add_plugin_source_for(&mut self, workspace: &str, cx: &mut Context<Self>) {
        let source = self
            .ws(workspace)
            .and_then(|w| w.plugin_source_draft.input.as_ref())
            .map(|c| c.read(cx).text().trim().to_owned())
            .unwrap_or_default();
        if source.is_empty() {
            return;
        }
        let Some(path) = self.ws(workspace).map(|ws| ws.path.clone()) else {
            return;
        };
        self.send_plugin_operation(
            workspace,
            "plugins/marketplace/add",
            add_marketplace_payload(&path, &source, None, None),
            cx,
        );
    }

    pub(crate) fn refresh_plugin_sources_for(&mut self, workspace: &str, cx: &mut Context<Self>) {
        let Some(path) = self.ws(workspace).map(|ws| ws.path.clone()) else {
            return;
        };
        self.send_plugin_operation(
            workspace,
            "plugins/marketplace/update",
            update_marketplace_payload(&path, None, None),
            cx,
        );
    }

    pub(crate) fn settle_plugin_action(
        &mut self,
        workspace: &str,
        method: &str,
        result: Option<&Value>,
        cx: &mut Context<Self>,
    ) {
        // RPC 成功不等于操作成功；diagnostics/refreshFailure 必须先校验再报告完成。
        match crate::shared::plugin_results::operation_result(method, result) {
            Ok(failures) if failures.is_empty() => {
                self.push_log(format!("plugin action completed: {method}"));
                if let Some(ws) = self.ws_mut(workspace) {
                    ws.inspection.plugin_descriptions.clear();
                    ws.pending.retain(|_, p| {
                        !matches!(p, crate::backend::workspace::Pending::PluginDescribe(_))
                    });
                }
                if let Some(form) = self
                    .ws_mut(workspace)
                    .and_then(|w| w.inspection.plugin_config.as_mut())
                {
                    form.needs_reload = true;
                    form.confirmation = false;
                    form.error = Some("Plugin changed; Reload scoped configuration".into());
                }
                self.fetch_plugins_overview_for(workspace, cx);
            }
            outcome => {
                let message = match outcome {
                    Ok(failures) => failures.join("\n"),
                    Err(error) => error,
                };
                if let Some(ws) = self.ws_mut(workspace) {
                    ws.inspection.plugin_operation_error =
                        Some(crate::shared::redact::scrub(&message));
                }
                self.status_error(workspace, &message);
                self.push_error(message);
            }
        }
    }
}
