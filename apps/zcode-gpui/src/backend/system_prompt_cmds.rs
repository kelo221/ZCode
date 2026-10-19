//! `workspace/readSystemPrompt` / `workspace/updateSystemPrompt` (GPUI-only,
//! see system_prompt/mod.rs). The CLI keeps the override in memory per
//! process, so every agent gets it again when its storage becomes ready.

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::system_prompt::DefaultPrompt;
use gpui::Context;
use serde_json::{Value, json};

const METHOD_NOT_FOUND: i64 = -32601;

impl AppState {
    fn system_prompt_workspace(&self, ws_key: &str) -> Option<Value> {
        let ws = self.ws(ws_key).filter(|ws| ws.started)?;
        Some(json!({ "workspacePath": ws.path, "workspaceKey": ws.key }))
    }

    fn send_system_prompt_update(&mut self, ws_key: &str) {
        let Some(workspace) = self.system_prompt_workspace(ws_key) else {
            return;
        };
        let prompt = self.system_prompt.override_text.clone();
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::SystemPrompt { read: false });
        let params = json!({ "workspace": workspace, "prompt": prompt });
        self.send_request(ws_key, "workspace/updateSystemPrompt", params, id);
    }

    /// Storage-ready hook: a fresh agent starts on the default, so only an
    /// override needs sending.
    pub(crate) fn sync_system_prompt(&mut self, ws_key: &str) {
        if self.system_prompt.override_text.is_some() {
            self.send_system_prompt_update(ws_key);
        }
    }

    /// Loads the default prompt from the active workspace's agent.
    pub(crate) fn read_system_prompt_default(&mut self, cx: &mut Context<Self>) {
        let Some(ws_key) = self.active_ws_key() else {
            return;
        };
        let Some(workspace) = self.system_prompt_workspace(&ws_key) else {
            return;
        };
        let Some(ws) = self.ws_mut(&ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::SystemPrompt { read: true });
        self.system_prompt.reading = true;
        self.send_request(
            &ws_key,
            "workspace/readSystemPrompt",
            json!({ "workspace": workspace }),
            id,
        );
        if !self
            .ws(&ws_key)
            .is_some_and(|ws| ws.pending.contains_key(&id))
        {
            self.system_prompt.reading = false;
        }
        cx.notify();
    }

    /// Apply / Reset: persist first, then push to every running agent.
    pub(crate) fn apply_system_prompt(&mut self, next: Option<String>, cx: &mut Context<Self>) {
        if let Err(error) = self.system_prompt.commit(next) {
            self.system_prompt.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let keys: Vec<String> = self.workspaces.iter().map(|ws| ws.key.clone()).collect();
        for key in keys {
            self.send_system_prompt_update(&key);
        }
        cx.notify();
    }

    pub(crate) fn handle_system_prompt_response(
        &mut self,
        ws_key: &str,
        id: u64,
        result: &Option<Value>,
        error: &Option<Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        let pending = self.ws(ws_key).and_then(|ws| ws.pending.get(&id));
        let Some(&Pending::SystemPrompt { read }) = pending else {
            return false;
        };
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.pending.remove(&id);
        }
        let state = &mut self.system_prompt;
        if read {
            state.reading = false;
        }
        match (result, error) {
            (_, Some(error)) => {
                if error.get("code").and_then(Value::as_i64) == Some(METHOD_NOT_FOUND) {
                    state.unavailable = true;
                } else {
                    let message = error.get("message").and_then(Value::as_str);
                    state.error = Some(message.unwrap_or("rpc error").to_owned());
                }
            }
            (Some(result), None) if read => {
                if let Some(text) = result.get("defaultPrompt").and_then(Value::as_str) {
                    state.default_prompt = Some(DefaultPrompt {
                        text: text.to_owned(),
                        from_session: result.get("defaultSource").and_then(Value::as_str)
                            == Some("session"),
                    });
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }
}
