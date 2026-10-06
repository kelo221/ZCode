use crate::app::store::AppState;
use crate::composer::{
    attachment::AttachmentRef,
    delivery::{InputRouting, SubmitTrigger},
};
use gpui::Context;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct HeldConfirmation {
    pub workspace: String,
    pub session: String,
    generation: u64,
    navigation: u64,
    replacement: u64,
    text: String,
    attachments: Vec<AttachmentRef>,
    config: Value,
    pub queue_ids: Vec<String>,
    pub trigger: SubmitTrigger,
}

#[derive(Clone, Copy)]
pub(crate) enum HeldDisposition {
    Clear,
    Keep,
}
impl HeldDisposition {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Self::Clear => "clearQueueAndSend",
            Self::Keep => "keepQueueAndSend",
        }
    }
}
pub(crate) struct HeldDecision {
    pub disposition: HeldDisposition,
    pub receipt: Arc<HeldConfirmation>,
}

impl AppState {
    fn held_config(&self) -> Value {
        let conversation = self.active_conversation();
        json!([
            self.submission_override(),
            conversation.map(|c| (
                &c.config.provider,
                &c.config.model,
                &c.config.thought,
                &c.config.mode,
                &c.config.followup_mode,
                c.can_stop
            ))
        ])
    }

    fn held_queue_ids(&self) -> Option<Vec<String>> {
        if self.draft
            || self.is_read_only_view()
            || self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send
        {
            return None;
        }
        let conv = self.active_conversation()?;
        if conv.input_routing != Some(InputRouting::Choice) {
            return None;
        }
        let mut ids = conv
            .queue
            .as_ref()?
            .items
            .iter()
            .map(|i| i.queue_item_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        if ids.is_empty()
            || ids.iter().any(|id| id.trim().is_empty())
            || ids.windows(2).any(|ids| ids[0] == ids[1])
        {
            return None;
        }
        Some(ids)
    }

    pub(crate) fn begin_held_confirmation(
        &mut self,
        trigger: SubmitTrigger,
        cx: &mut Context<Self>,
    ) -> bool {
        let composer = self.composer.read(cx);
        if !matches!(
            crate::composer::slash::classify_slash_command(composer.text().trim()),
            crate::composer::slash::SlashAction::Plain(_)
                | crate::composer::slash::SlashAction::Goal(_)
                | crate::composer::slash::SlashAction::Plan(_)
        ) || composer.text().trim().is_empty() && composer.attachments.is_empty()
        {
            return false;
        }
        let Some(queue_ids) = self.held_queue_ids() else {
            return false;
        };
        let Some(workspace) = self.active_ws_key() else {
            return false;
        };
        let Some(ws) = self
            .ws(&workspace)
            .filter(|w| w.started && w.inbound.is_some())
        else {
            return false;
        };
        self.held_confirmation = Some(Arc::new(HeldConfirmation {
            workspace,
            session: self.active.clone().unwrap(),
            generation: ws.generation,
            navigation: self.navigation_generation,
            replacement: composer.replacement_generation,
            text: composer.text().into(),
            attachments: composer.attachments.clone(),
            config: self.held_config(),
            queue_ids,
            trigger,
        }));
        cx.notify();
        true
    }

    pub(crate) fn held_binding_matches(&self, receipt: &HeldConfirmation, cx: &gpui::App) -> bool {
        let composer = self.composer.read(cx);
        self.active_ws_key().as_deref() == Some(&receipt.workspace)
            && self.active.as_deref() == Some(&receipt.session)
            && self.navigation_generation == receipt.navigation
            && self.ws(&receipt.workspace).is_some_and(|w| {
                w.started && w.inbound.is_some() && w.generation == receipt.generation
            })
            && self.held_queue_ids().is_some()
            && composer.replacement_generation == receipt.replacement
            && composer.text() == receipt.text
            && composer.attachments == receipt.attachments
            && self.held_config() == receipt.config
    }

    pub(crate) fn cancel_held_confirmation(
        &mut self,
        receipt: &Arc<HeldConfirmation>,
        cx: &mut Context<Self>,
    ) {
        if self
            .held_confirmation
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, receipt))
        {
            self.held_confirmation = None;
            cx.notify();
        }
    }

    pub(crate) fn confirm_held_submission(
        &mut self,
        receipt: Arc<HeldConfirmation>,
        disposition: HeldDisposition,
        cx: &mut Context<Self>,
    ) {
        if !self
            .held_confirmation
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &receipt))
        {
            return;
        }
        self.held_confirmation = None;
        if !self.held_binding_matches(&receipt, cx) {
            cx.notify();
            return;
        }
        if self.held_queue_ids().as_ref() != Some(&receipt.queue_ids) {
            // 队列集合已变更时重新确认，不让旧 Clear 删除用户未审阅的新输入。
            self.begin_held_confirmation(receipt.trigger, cx);
            return;
        }
        let delivery = self.opposite_delivery(receipt.trigger);
        self.transfer_composer_submission(
            delivery,
            Some(HeldDecision {
                disposition,
                receipt,
            }),
            cx,
        );
    }

    pub(crate) fn settle_held_submission(
        &mut self,
        workspace: &str,
        submission: crate::backend::submission::Submission,
        trigger: SubmitTrigger,
        ack: Option<&Value>,
        cx: &mut Context<Self>,
    ) {
        let reopen = ack.is_some_and(|a| {
            a["status"] == "failed" && a["reasonCode"] == "guard.heldQueueConfirmationStale"
        }) && self.navigation_generation == submission.navigation_generation
            && self.active_ws_key().as_deref() == Some(workspace)
            && self.active.as_deref() == Some(&submission.draft_key);
        self.handle_submission_ack(workspace, submission, ack, false, cx);
        if reopen {
            self.begin_held_confirmation(trigger, cx);
        }
    }
}

#[cfg(test)]
#[path = "held_queue_tests.rs"]
mod tests;
