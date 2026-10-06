//! User actions on sessions, rows, the queue and pending interactions, plus
//! the composer intent (send / edit message / rename session) and per-session
//! drafts. Every command goes through `send_session_command` so its ack is
//! checked (backend/session_cmds.rs).
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts.

use crate::app::store::AppState;
use crate::backend::session_cmds::row_payload;
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
        // 同一个新会话草稿可被再次打开；key 相同不能让旧 create ACK 抢走新导航。
        self.navigation_generation = self
            .navigation_generation
            .checked_add(1)
            .expect("navigation generation exhausted");
        if self.composer_intent != ComposerIntent::Send {
            self.composer_intent = ComposerIntent::Send;
            return;
        }
        let key = self.draft_key();
        let (attachments, owned) = self
            .composer
            .update(cx, |c, _| (c.take_attachments(), c.drain_temp_ownership()));
        self.recovered_attachments.insert(key.clone(), attachments);
        self.retired_temp_files.extend(owned);
        let cur = self.composer.read(cx).text().to_string();
        if !cur.trim().is_empty() {
            self.session_drafts.insert(key, cur);
        } else {
            self.session_drafts.remove(&key);
        }
    }

    pub(crate) fn restore_draft(&mut self, key: &str, cx: &mut Context<Self>) {
        let restored = self.session_drafts.get(key).cloned().unwrap_or_default();
        let attachments = self.recovered_attachments.remove(key).unwrap_or_default();
        self.composer.update(cx, |c, cx| {
            c.set_text(&restored);
            c.attachments = attachments;
            cx.notify();
        });
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

    /// Fork conversation branch from a completed assistant turn.
    pub fn fork_assistant(&mut self, row_id: u64, entity_id: &str, cx: &mut Context<Self>) {
        self.row_command("forkAssistant", row_id, entity_id, json!({}));
        cx.notify();
    }

    /// Provide like/dislike/clear feedback on an assistant turn.
    pub fn set_assistant_feedback(
        &mut self,
        row_id: u64,
        entity_id: &str,
        feedback: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let fb_val = feedback.map(|f| json!(f)).unwrap_or(Value::Null);
        self.row_command(
            "setAssistantFeedback",
            row_id,
            entity_id,
            json!({ "feedback": fb_val }),
        );
        cx.notify();
    }

    /// Workspace-only file undo; the UI asks for confirmation first.
    pub fn apply_file_rewind(&mut self, row_id: u64, entity_id: &str, cx: &mut Context<Self>) {
        self.row_command("applyFileRewind", row_id, entity_id, json!({}));
        cx.notify();
    }

    /// Compact conversation history into a concise summary.
    pub fn compact_session(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return false;
        };
        let ctx = CommandCtx::new(&sid, "compact", json!({}));
        let ok = self.send_session_command(&ws_key, ctx);
        cx.notify();
        ok
    }

    /// Pause the active goal execution (stopPausesActiveGoalTarget).
    #[allow(dead_code)]
    pub fn pause_goal(&mut self, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        self.send_session_command(&ws_key, CommandCtx::new(&sid, "pauseGoal", json!({})).cas());
        cx.notify();
    }

    /// Resume a paused goal execution.
    #[allow(dead_code)]
    pub fn resume_goal(&mut self, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        self.send_session_command(
            &ws_key,
            CommandCtx::new(&sid, "resumeGoal", json!({})).cas(),
        );
        cx.notify();
    }

    /// Set session-level followup routing mode ("queue" or "guide").
    #[allow(dead_code)]
    pub fn set_followup_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        self.send_session_command(
            &ws_key,
            CommandCtx::new(&sid, "setFollowupMode", json!({ "mode": mode })).cas(),
        );
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

    #[allow(dead_code)]
    pub fn edit_queue_item(&mut self, queue_item_id: &str, new_text: &str, cx: &mut Context<Self>) {
        self.queue_command(
            "editQueueItem",
            json!({ "queueItemId": queue_item_id, "newText": new_text }),
        );
        cx.notify();
    }

    pub fn reorder_queue_item(
        &mut self,
        queue_item_id: &str,
        before_queue_item_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let before_val = before_queue_item_id
            .map(|b| json!(b))
            .unwrap_or(Value::Null);
        self.queue_command(
            "reorderQueueItem",
            json!({ "queueItemId": queue_item_id, "beforeQueueItemId": before_val }),
        );
        cx.notify();
    }

    /// Cancel an in-flight background task or workflow run.
    #[cfg(test)]
    pub fn cancel_background_work(&mut self, work_id: &str, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        self.cancel_background_work_for(&ws_key, &sid, work_id, cx);
    }
}
