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
            ws.pending.remove(&id);
            return;
        }
        if !ws.send_line(json!({"id": id, "method": method, "params": params}).to_string()) {
            ws.pending.remove(&id);
        }
    }

    pub(crate) fn send_command(
        &mut self,
        ws_key: &str,
        sid: Option<String>,
        ctype: &str,
        payload: Value,
        base_revision: Option<u64>,
        pending: Pending,
    ) -> bool {
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
        self.send_envelope(ws_key, params, pending)
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
        if !ws.send_line(json!({"id": id, "method": "v4/command", "params": params}).to_string()) {
            ws.pending.remove(&id);
            self.push_error("Workspace agent input closed; your message was not sent".into());
            return false;
        }
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
        self.clear_subagent_view();
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
        self.clear_subagent_view();
        self.ensure_spawned(ws_key, cx);
        self.active_workspace = Some(ws_key.to_string());
        self.active = None;
        self.draft = true;
        self.ui_model_value = None;
        self.ui_mode = None;
        self.restore_draft(&format!("draft:{ws_key}"), cx);
        cx.notify();
    }

    /// Start a new chat in the dedicated Tasks (conversation) workspace,
    /// creating chats without a project context.
    /// Desktop parity: packages/ui/src/root/useConversationWorkspaceActions.ts
    /// `handleCreateConversationTask`.
    pub fn new_conversation_chat(&mut self, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.clear_subagent_view();
        if let Some(key) = self.conversation_workspace_key() {
            self.ensure_spawned(&key, cx);
            self.active_workspace = Some(key.clone());
            self.active = None;
            self.draft = true;
            self.ui_model_value = None;
            self.ui_mode = None;
            self.restore_draft(&format!("draft:{key}"), cx);
            cx.notify();
        }
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
