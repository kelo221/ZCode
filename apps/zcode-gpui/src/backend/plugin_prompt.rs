use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::composer::{
    attachment::AttachmentRef,
    references::{CatalogKind, parse_catalog},
};
use gpui::Context;
use serde_json::{Value, json};

pub(crate) struct PluginPrompt {
    pub plugin: String,
    pub prompt: String,
    pub generation: u64,
    pub text: String,
    pub attachments: Vec<AttachmentRef>,
}
impl AppState {
    fn plugin_prompt_draft_empty(&self, key: &str, cx: &gpui::App) -> bool {
        let draft = format!("draft:{key}");
        !self
            .session_drafts
            .get(&draft)
            .is_some_and(|t| !t.trim().is_empty())
            && !self
                .recovered_attachments
                .get(&draft)
                .is_some_and(|a| !a.is_empty())
            && !self.draft_submission_overrides.contains_key(&draft)
            && (self.active.is_some()
                || self.composer.read(cx).text().trim().is_empty()
                    && self.composer.read(cx).attachments().is_empty())
    }
    pub(crate) fn plugin_prompt_pending(&self, key: &str) -> bool {
        self.ws(key).is_some_and(|w| {
            w.pending
                .values()
                .any(|p| matches!(p, Pending::PluginPrompt(_)))
        })
    }
    pub(crate) fn use_plugin_prompt(
        &mut self,
        key: &str,
        plugin: &str,
        prompt: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_operation_available(key)
            || self.plugin_prompt_pending(key)
            || self.is_read_only_view()
            || self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send
        {
            return;
        }
        let entry = self
            .ws(key)
            .and_then(|w| w.inspection.plugins.value.as_ref())
            .and_then(|v| v.available_plugins.iter().find(|p| p.id == plugin))
            .cloned();
        let Some(entry) = entry else { return };
        if !entry.installed {
            self.install_plugin_for(key, &entry.name, &entry.marketplace, cx);
            return;
        }
        if !self.plugin_prompt_draft_empty(key, cx) {
            self.plugin_prompt_error(key, "A project draft already exists; preserve or send it before using an example prompt");
            cx.notify();
            return;
        }
        let pending = PluginPrompt {
            plugin: plugin.into(),
            prompt: prompt.trim().into(),
            generation: self.navigation_generation,
            text: self.composer.read(cx).text().into(),
            attachments: self.composer.read(cx).attachments().to_vec(),
        };
        let ws = self.ws_mut(key).unwrap();
        ws.inspection.plugin_operation_error = None;
        let id = ws.next_id();
        ws.pending.insert(id, Pending::PluginPrompt(pending));
        if !ws.send_pending_line(id, json!({"id":id,"method":"plugins/referenceCatalog","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key}}}).to_string()) {
            self.plugin_prompt_error(key, "Example prompt reference could not be requested");
        }
        cx.notify();
    }
    pub(crate) fn plugin_prompt_error(&mut self, key: &str, error: &str) {
        if let Some(ws) = self.ws_mut(key) {
            ws.inspection.plugin_operation_error = Some(crate::shared::redact::scrub(error));
        }
    }
    pub(crate) fn settle_plugin_prompt(
        &mut self,
        key: &str,
        prompt: PluginPrompt,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let entries = match result.and_then(|v| parse_catalog(v, CatalogKind::Plugins, false)) {
            Ok(entries) => entries,
            Err(error) => {
                self.plugin_prompt_error(key, &error);
                return;
            }
        };
        if self.active_ws_key().as_deref() != Some(key)
            || self.navigation_generation != prompt.generation
            || self.is_read_only_view()
            || self.composer.read(cx).text() != prompt.text
            || self.composer.read(cx).attachments() != prompt.attachments
            || !self.plugin_prompt_draft_empty(key, cx)
            || self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send
        {
            return;
        }
        let destination = format!("plugin://{}", prompt.plugin);
        let entry = entries
            .into_iter()
            .find(|e| e.insert_text.ends_with(&format!("({destination})")));
        let Some(entry) = entry else {
            self.plugin_prompt_error(key, "Plugin reference is missing, disabled or conflicting");
            return;
        };
        // 示例只能预填新 draft；旧实现直接 set_text 会丢失当前会话草稿与附件归属。
        self.set_active_workspace(key, cx);
        self.composer.update(cx, |c, cx| {
            c.set_text("");
            c.insert_chip(0..0, &entry.insert_text);
            if !prompt.prompt.is_empty() {
                let end = c.text().len();
                c.edit_range(end..end, &prompt.prompt);
            }
            c.caret = c.text().len();
            cx.notify();
        });
        cx.notify();
    }
}
