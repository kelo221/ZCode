use crate::app::store::AppState;
use crate::backend::{submission::Submission, workspace::Pending};
use crate::composer::held_queue::HeldDecision;
use gpui::Context;
use serde_json::json;

impl AppState {
    pub fn send_goal_command(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        self.send_goal_command_with_held(text, None, cx)
    }

    pub(crate) fn send_goal_command_with_held(
        &mut self,
        text: &str,
        held: Option<HeldDecision>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return false;
        };
        let mut payload = self.with_submission_override(json!({ "text": text }));
        let submission = Submission {
            workspace: ws_key.clone(),
            draft_key: sid.clone(),
            navigation_generation: self.navigation_generation,
            text: format!("/goal {text}"),
            attachments: vec![],
            config: self.submission_override(),
        };
        let pending = if let Some(held) = held {
            payload["heldQueueDisposition"] = json!(held.disposition.wire());
            payload["expectedHeldQueueItemIds"] = json!(held.receipt.queue_ids);
            Pending::HeldSend {
                submission,
                trigger: held.receipt.trigger,
            }
        } else {
            Pending::SendText(submission)
        };
        let ok = self.send_command(
            &ws_key,
            Some(sid),
            "sendGoalCommand",
            payload,
            None,
            pending,
        );
        if ok {
            self.draft_submission_overrides
                .remove(&self.composer_draft_key());
        }
        cx.notify();
        ok
    }
}
