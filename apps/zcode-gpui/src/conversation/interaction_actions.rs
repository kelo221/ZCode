//! Answering pending interactions (permission requests, agent questions).
//! Split from msg_actions.rs for the 400-line cap.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts.

use crate::app::store::AppState;
use crate::backend::workspace::CommandCtx;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
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
        // Idempotent by interactionId: a second click (same frame, or on a
        // re-announced card) must not send a second answer.
        let Some(state) = self.conversations.get_mut(&sid) else {
            return;
        };
        if !state.resolving.begin(interaction_id) {
            return;
        }
        let payload = json!({ "interactionId": interaction_id, "answer": answer });
        let ctx = CommandCtx::new(&sid, "resolveInteraction", payload);
        let sent = self.send_session_command(&ws_key, ctx);
        if let Some(state) = self.conversations.get_mut(&sid) {
            if sent {
                state
                    .pending_interactions
                    .retain(|pi| pi.interaction_id != interaction_id);
            } else {
                state.resolving.release(interaction_id);
            }
        }
        cx.notify();
    }
}
