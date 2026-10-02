//! User actions on sessions, rows, the queue and pending interactions, plus
//! the composer intent (send / edit message / rename session) and per-session
//! drafts. Every command goes through `send_session_command` so its ack is
//! checked (backend/session_cmds.rs).
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts.

use crate::backend::session_cmds::row_payload;
use crate::app::store::AppState;
use crate::backend::workspace::CommandCtx;
use gpui::Context;
use serde_json::{Value, json};

/// What Enter in the composer does. Edit and rename borrow the composer as
/// their text field; the user's own draft is restored afterwards.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ComposerIntent {
    #[default]
    Send,
    Edit {
        ws_key: String,
        sid: String,
        row_id: u64,
        entity_id: String,
    },
    Rename {
        ws_key: String,
        sid: String,
    },
}

impl AppState {
    fn draft_key(&self) -> String {
        self.active
            .clone()
            .unwrap_or_else(|| format!("draft:{}", self.active_workspace.as_deref().unwrap_or("")))
    }

    /// Save the composer text as the current context's draft. Edit/rename
    /// text is never a draft: navigating away cancels that intent instead.
    pub(crate) fn save_current_draft(&mut self, cx: &mut Context<Self>) {
        if self.composer_intent != ComposerIntent::Send {
            self.composer_intent = ComposerIntent::Send;
            return;
        }
        let key = self.draft_key();
        let cur = self.composer.read(cx).text().to_string();
        if !cur.trim().is_empty() {
            self.session_drafts.insert(key, cur);
        } else {
            self.session_drafts.remove(&key);
        }
    }

    pub(crate) fn restore_draft(&mut self, key: &str, cx: &mut Context<Self>) {
        let restored = self.session_drafts.get(key).cloned().unwrap_or_default();
        self.composer.update(cx, |c, _ccx| c.set_text(&restored));
    }

    // ── Row actions (CAS + baseLogEpoch) ──

    fn row_command(&mut self, ctype: &str, row_id: u64, entity_id: &str, extra: Value) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let Some(epoch) = self.row_epoch(&sid) else {
            return;
        };
        let payload = row_payload(row_id, entity_id, extra);
        self.send_session_command(&ws_key, CommandCtx::new(&sid, ctype, payload).row(epoch));
    }

    pub fn retry_turn(&mut self, row_id: u64, entity_id: &str, cx: &mut Context<Self>) {
        self.row_command("retryTurn", row_id, entity_id, json!({}));
        cx.notify();
    }

    /// Workspace-only file undo; the UI asks for confirmation first.
    pub fn apply_file_rewind(&mut self, row_id: u64, entity_id: &str, cx: &mut Context<Self>) {
        self.row_command("applyFileRewind", row_id, entity_id, json!({}));
        cx.notify();
    }

    // ── Composer intents ──

    pub fn begin_edit(&mut self, row_id: u64, entity_id: &str, text: &str, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        self.save_current_draft(cx);
        self.composer_intent = ComposerIntent::Edit {
            ws_key,
            sid,
            row_id,
            entity_id: entity_id.to_string(),
        };
        self.composer.update(cx, |c, _ccx| c.set_text(text));
        cx.notify();
    }

    pub fn begin_rename(&mut self, ws_key: &str, sid: &str, title: &str, cx: &mut Context<Self>) {
        self.save_current_draft(cx);
        self.composer_intent = ComposerIntent::Rename {
            ws_key: ws_key.to_string(),
            sid: sid.to_string(),
        };
        self.composer.update(cx, |c, _ccx| c.set_text(title));
        cx.notify();
    }

    pub fn cancel_intent(&mut self, cx: &mut Context<Self>) {
        self.composer_intent = ComposerIntent::Send;
        let key = self.draft_key();
        self.restore_draft(&key, cx);
        cx.notify();
    }

    /// Enter while editing/renaming: dispatch, then give the draft back. If
    /// the command can't be sent (agent offline, conversation still loading)
    /// the intent and its text stay in the composer so nothing is lost.
    pub(crate) fn submit_intent(&mut self, text: &str, cx: &mut Context<Self>) {
        let intent = std::mem::take(&mut self.composer_intent);
        let sent = match &intent {
            ComposerIntent::Send => true,
            ComposerIntent::Edit {
                ws_key,
                sid,
                row_id,
                entity_id,
            } => match self.row_epoch(sid) {
                Some(epoch) => {
                    let extra = json!({ "newText": text, "workspaceMode": "preserve" });
                    let payload = row_payload(*row_id, entity_id, extra);
                    let ctx = CommandCtx::new(sid, "editUserQuery", payload).row(epoch);
                    self.send_session_command(ws_key, ctx)
                }
                None => false,
            },
            ComposerIntent::Rename { ws_key, sid } => self.rename_session(ws_key, sid, text),
        };
        if sent {
            let key = self.draft_key();
            self.restore_draft(&key, cx);
        } else {
            self.composer_intent = intent;
            self.composer.update(cx, |c, _ccx| c.set_text(text));
        }
        cx.notify();
    }

    // ── Session management (owner workspace passed explicitly) ──

    pub fn rename_session(&mut self, ws_key: &str, sid: &str, title: &str) -> bool {
        let payload = json!({ "title": title });
        if !self.send_session_command(ws_key, CommandCtx::new(sid, "renameSession", payload)) {
            return false;
        }
        if let Some(entry) = self
            .ws_mut(ws_key)
            .and_then(|ws| ws.sessions.iter_mut().find(|s| s.session_id == sid))
        {
            entry.title = title.to_string();
        }
        true
    }

    /// User-initiated delete (the sidebar asks for confirmation first).
    pub fn remove_session(&mut self, ws_key: &str, sid: &str, cx: &mut Context<Self>) {
        let ctx = CommandCtx::new(sid, "deleteSession", json!({}));
        if !self.send_session_command(ws_key, ctx) {
            return;
        }
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.sessions.retain(|s| s.session_id != sid);
        }
        self.conversations.remove(sid);
        self.session_drafts.remove(sid);
        let intent_targets_sid = matches!(
            &self.composer_intent,
            ComposerIntent::Edit { sid: s, .. } | ComposerIntent::Rename { sid: s, .. } if s == sid
        );
        if intent_targets_sid {
            self.composer_intent = ComposerIntent::Send;
        }
        if self.active.as_deref() == Some(sid) {
            // Land on a new-chat draft in the same project rather than
            // silently opening an unrelated (unsubscribed) session.
            self.active = None;
            self.draft = true;
            self.active_workspace = Some(ws_key.to_string());
            self.composer_intent = ComposerIntent::Send;
            self.restore_draft(&format!("draft:{ws_key}"), cx);
        }
        cx.notify();
    }

    // ── Follow-up queue (CAS) ──

    fn queue_command(&mut self, ctype: &str, payload: Value) -> bool {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return false;
        };
        self.send_session_command(&ws_key, CommandCtx::new(&sid, ctype, payload).cas())
    }

    pub fn send_queued_now(&mut self, queue_item_id: &str, cx: &mut Context<Self>) {
        self.queue_command("sendQueuedNow", json!({ "queueItemId": queue_item_id }));
        cx.notify();
    }

    pub fn delete_queue_item(&mut self, queue_item_id: &str, cx: &mut Context<Self>) {
        if self.queue_command("deleteQueueItem", json!({ "queueItemId": queue_item_id }))
            && let Some(q) = self
                .active
                .clone()
                .and_then(|sid| self.conversations.get_mut(&sid))
                .and_then(|c| c.queue.as_mut())
        {
            q.items.retain(|it| it.queue_item_id != queue_item_id);
        }
        cx.notify();
    }

    pub fn set_auto_drain(&mut self, auto_drain: bool, cx: &mut Context<Self>) {
        if self.queue_command("setAutoDrain", json!({ "autoDrain": auto_drain }))
            && let Some(q) = self
                .active
                .clone()
                .and_then(|sid| self.conversations.get_mut(&sid))
                .and_then(|c| c.queue.as_mut())
        {
            q.auto_drain = auto_drain;
        }
        cx.notify();
    }

    // ── Interactions ──

    /// Answer a pending interaction through V4 `resolveInteraction` only.
    /// The backend races the legacy stdio request against this command and
    /// cancels the stdio request when the V4 answer lands, so the stdio
    /// request is deliberately never answered here: a stdio reply would win
    /// the race with less information (Allow always / Full access lost).
    pub fn resolve_interaction(
        &mut self,
        interaction_id: &str,
        answer: Value,
        cx: &mut Context<Self>,
    ) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let payload = json!({ "interactionId": interaction_id, "answer": answer });
        let ctx = CommandCtx::new(&sid, "resolveInteraction", payload);
        if self.send_session_command(&ws_key, ctx)
            && let Some(state) = self.conversations.get_mut(&sid)
        {
            state
                .pending_interactions
                .retain(|pi| pi.interaction_id != interaction_id);
        }
        cx.notify();
    }

    /// Composer text used as an interaction's free-text answer/feedback;
    /// clears the composer on use.
    pub(crate) fn take_composer_answer(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let text = self.composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            self.push_error("Type your answer in the message box first".into());
            cx.notify();
            return None;
        }
        self.composer.update(cx, |c, _ccx| c.set_text(""));
        Some(text)
    }
}
