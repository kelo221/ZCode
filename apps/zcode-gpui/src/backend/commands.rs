//! Outbound actions on AppState: RPC plumbing, topic subscriptions and chat
//! commands. Config switches (model/thinking/mode) live in composer/config_cmds.rs.

use crate::app::store::AppState;
use crate::backend::launcher::{new_command_id, now_ms};
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn send_request(&mut self, ws_key: &str, method: &str, params: Value, id: u64) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        if ws.inbound.is_none() {
            ws.status = "not connected".into();
            return;
        }
        ws.send_line(json!({"id": id, "method": method, "params": params}).to_string());
    }

    pub(crate) fn send_command(
        &mut self,
        ws_key: &str,
        sid: Option<String>,
        ctype: &str,
        payload: Value,
        base_revision: Option<u64>,
        pending: Pending,
    ) {
        let params = crate::backend::session_cmds::command_params(
            &self.client_id,
            sid.as_deref(),
            ctype,
            payload,
            base_revision,
            None,
            new_command_id(),
            now_ms(),
        );
        self.send_envelope(ws_key, params, pending);
    }

    /// Write one `v4/command` envelope and register its pending entry.
    /// Returns false (and surfaces an error) when the agent isn't connected.
    pub(crate) fn send_envelope(&mut self, ws_key: &str, params: Value, pending: Pending) -> bool {
        let Some(ws) = self.ws_mut(ws_key) else {
            return false;
        };
        if ws.inbound.is_none() {
            ws.status = "not connected".into();
            self.push_error("Workspace agent is not connected; try again in a moment".into());
            return false;
        }
        let id = ws.next_id();
        ws.pending.insert(id, pending);
        ws.send_line(json!({"id": id, "method": "v4/command", "params": params}).to_string());
        true
    }

    pub(crate) fn subscribe_index(&mut self, ws_key: &str) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::SubscribeIndex);
        let key = ws.key.clone();
        let conn_id = ws.connection_id.clone();
        self.send_request(
            ws_key,
            "v4/conversation/subscribe",
            json!({
                "topic": format!("sessions-index/{}", key),
                "connectionId": conn_id,
                "clientMode": "desktop-continuous",
                "visibility": "foreground",
            }),
            id,
        );
    }

    pub(crate) fn subscribe_config(&mut self, ws_key: &str) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::SubscribeConfig);
        let key = ws.key.clone();
        let conn_id = ws.connection_id.clone();
        self.send_request(
            ws_key,
            "v4/conversation/subscribe",
            json!({
                "topic": format!("workspace-config/{}", key),
                "connectionId": conn_id,
                "clientMode": "desktop-continuous",
                "visibility": "foreground",
            }),
            id,
        );
    }

    /// Harvest the model catalog via a legacy session/create (the only stdio
    /// surface carrying the full list), deleting the session it creates.
    pub(crate) fn probe_model_catalog(&mut self, ws_key: &str) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::ModelCatalog);
        let key = ws.key.clone();
        self.send_request(
            ws_key,
            "session/create",
            json!({ "workspace": { "workspacePath": key, "workspaceKey": key } }),
            id,
        );
    }

    pub(crate) fn delete_session(&mut self, ws_key: &str, sid: &str) {
        self.send_command(
            ws_key,
            Some(sid.to_string()),
            "deleteSession",
            json!({}),
            None,
            Pending::CleanupCatalog,
        );
    }

    pub(crate) fn subscribe_conversation(&mut self, ws_key: &str, sid: &str) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::SubscribeConversation(sid.to_string()));
        let key = ws.key.clone();
        let conn_id = ws.connection_id.clone();
        self.send_request(
            ws_key,
            "v4/conversation/subscribe",
            json!({
                "topic": format!("conversation/{sid}"),
                "connectionId": conn_id,
                "clientMode": "desktop-continuous",
                "visibility": "foreground",
                "workspace": {
                    "workspacePath": key,
                    "workspaceKey": key,
                },
            }),
            id,
        );
    }

    /// Spawn this workspace's agent if it's idle (lazy start) and mark it
    /// recently used so the reaper leaves it alone.
    pub(crate) fn ensure_spawned(&mut self, ws_key: &str, cx: &mut Context<Self>) {
        let need = self
            .ws(ws_key)
            .map(|w| !w.pumping && w.inbound.is_none())
            .unwrap_or(false);
        if need {
            self.spawn_workspace(ws_key, cx);
        } else if let Some(ws) = self.ws_mut(ws_key) {
            ws.last_activity = Some(std::time::Instant::now());
        }
    }

    pub fn select_session(&mut self, ws_key: &str, sid: &str, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.active = Some(sid.to_string());
        self.active_workspace = Some(ws_key.to_string());
        self.draft = false;
        self.ui_model_value = None;
        self.ui_mode = None;
        self.restore_draft(sid, cx);

        self.ensure_spawned(ws_key, cx);
        let ready = self
            .ws(ws_key)
            .map(|w| w.started && w.inbound.is_some())
            .unwrap_or(false);
        let needs_sub = match self.conversations.get(sid) {
            Some(c) => !c.subscribed,
            None => true,
        };
        if ready {
            if needs_sub {
                self.subscribe_conversation(ws_key, sid);
            }
        } else if needs_sub {
            // Agent still starting: remember the target and subscribe once
            // storage reports ready.
            if let Some(ws) = self.ws_mut(ws_key) {
                ws.desired_conversation = Some(sid.to_string());
            }
        }
        cx.notify();
    }

    pub fn set_active_workspace(&mut self, ws_key: &str, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.ensure_spawned(ws_key, cx);
        self.active_workspace = Some(ws_key.to_string());
        self.active = None;
        self.draft = true;
        self.ui_model_value = None;
        self.ui_mode = None;
        self.restore_draft(&format!("draft:{ws_key}"), cx);
        cx.notify();
    }

    pub fn new_chat(&mut self, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        if let Some(key) = self.active_ws_key() {
            self.ensure_spawned(&key, cx);
        }
        self.active = None;
        self.draft = true;
        self.ui_model_value = None;
        self.ui_mode = None;
        let draft_key = format!("draft:{}", self.active_workspace.as_deref().unwrap_or(""));
        self.restore_draft(&draft_key, cx);
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn send(&mut self, text: &str, cx: &mut Context<Self>) {
        self.send_with_attachments(text, Vec::new(), cx);
    }

    pub fn send_with_attachments(
        &mut self,
        text: &str,
        attachments: Vec<crate::composer::attachment::AttachmentRef>,
        cx: &mut Context<Self>,
    ) {
        let text = text.trim();
        if text.is_empty() && attachments.is_empty() {
            return;
        }
        if self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send {
            self.submit_intent(text, cx);
            return;
        }

        // Handle slash commands (/compact, /goal <desc>, etc.)
        if text.starts_with('/') {
            match crate::composer::slash::classify_slash_command(text) {
                crate::composer::slash::SlashAction::Compact => {
                    if self.active.is_some() {
                        self.compact_session(cx);
                        return;
                    }
                }
                crate::composer::slash::SlashAction::Goal(desc) => {
                    if self.active.is_some() {
                        self.send_goal_command(&desc, cx);
                        return;
                    }
                }
                crate::composer::slash::SlashAction::Plain(_) => {}
            }
        }

        let Some(ws_key) = self.active_ws_key() else {
            self.push_log("no workspace selected".into());
            return;
        };

        let draft_key = self
            .active
            .clone()
            .unwrap_or_else(|| format!("draft:{}", self.active_workspace.as_deref().unwrap_or("")));
        self.session_drafts.remove(&draft_key);
        self.ensure_spawned(&ws_key, cx);
        let ready = self
            .ws(&ws_key)
            .map(|w| w.started && w.inbound.is_some())
            .unwrap_or(false);
        if !ready {
            self.push_log("backend still starting — try again in a moment".into());
            cx.notify();
            return;
        }
        if self.draft || self.active.is_none() {
            // Draft selections (model/mode) ride into createSession.config —
            // CAS config commands need a live session.
            let config = self.draft_config();
            let mut first_input = json!({ "text": text });
            if !attachments.is_empty() {
                first_input["attachments"] = json!(attachments);
            }
            let mut payload = json!({
                "workspaceId": self.ws(&ws_key).map(|w| w.key.clone()).unwrap_or_default(),
                "firstInput": first_input,
            });
            if !config.is_null() {
                payload["config"] = config;
            }
            self.send_command(
                &ws_key,
                None,
                "createSession",
                payload,
                None,
                Pending::CreateSession,
            );
            self.push_log("creating session…".into());
        } else if let Some(sid) = self.active.clone() {
            let mut text_payload = json!({ "text": text });
            if !attachments.is_empty() {
                text_payload["attachments"] = json!(attachments);
            }
            if let Some(c) = self.conversations.get(&sid)
                && c.config.followup_mode == "guide"
            {
                text_payload["requestedDelivery"] = json!("guide");
            }
            self.send_command(
                &ws_key,
                Some(sid),
                "sendText",
                text_payload,
                None,
                Pending::SendText,
            );
        }
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) {
            self.ensure_spawned(&ws_key, cx);
            let mut payload = json!({});
            if let Some(c) = self.conversations.get(&sid)
                && let Some(fg_id) = &c.active_foreground_execution_id
            {
                payload["expectedForegroundExecutionId"] = json!(fg_id);
            }
            self.send_command(&ws_key, Some(sid), "stop", payload, None, Pending::Stop);
            cx.notify();
        }
    }
}
