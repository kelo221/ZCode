//! Interaction cards: permission requests, agent questions (single/multi
//! question, multi-select), and typed answers/feedback from the card's input
//! (conversation/interaction_input.rs).

use crate::app::root::RootView;
use crate::conversation::interactions::{
    FULL_ACCESS_OPTION_ID, PendingInteraction, decline_answer, option_answer,
};
use crate::shared::theme::{ACCENT, AMBER, BORDER, CARD, DANGER, MUTED, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Context, Div, FontWeight, SharedString, Stateful, div, prelude::*};
use serde_json::Value;

pub(crate) fn action_btn(id: String, label: &str, bg: u32, fg: u32) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .min_w_0()
        .max_w_full()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .rounded_sm()
        .cursor_pointer()
        .child(label.to_string())
}

impl RootView {
    /// Answer button: settles the interaction with a fixed answer.
    pub(crate) fn answer_btn(
        &self,
        iid: &str,
        key: &str,
        label: &str,
        colors: (u32, u32),
        answer: Value,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let iid = iid.to_string();
        action_btn(format!("ia-{iid}-{key}"), label, colors.0, colors.1).on_click(cx.listener(
            move |this, _, _, cx| {
                cx.stop_propagation();
                this.clear_interaction_drafts(&iid, cx);
                let answer = answer.clone();
                let iid = iid.clone();
                this.state
                    .update(cx, |app, cx| app.resolve_interaction(&iid, answer, cx));
            },
        ))
    }

    /// Sends the text typed in the card's input (answer, plan change request
    /// or deny feedback).
    pub(crate) fn typed_answer_btn(
        &self,
        pi: &PendingInteraction,
        label: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pi = pi.clone();
        action_btn(format!("ia-{}-text", pi.interaction_id), label, CARD, TEXT)
            .border_1()
            .border_color(rgb(BORDER))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.send_typed_answer_or_hint(&pi, cx);
            }))
    }

    pub(crate) fn interaction_cards(
        &self,
        interactions: &[PendingInteraction],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        interactions
            .iter()
            .map(|pi| self.interaction_card(pi, cx))
            .collect()
    }

    fn interaction_card(&self, pi: &PendingInteraction, cx: &mut Context<Self>) -> AnyElement {
        if pi.is_plan_approval() {
            return self.plan_approval_card(pi, cx);
        }
        let iid = pi.interaction_id.as_str();
        let title = match pi.kind.as_str() {
            "permission" if !pi.tool_name.is_empty() => {
                format!("Permission request: {}", pi.tool_name)
            }
            "permission" => "Permission request".to_string(),
            "workspaceHookReview" => "Workspace hook review".to_string(),
            _ => "Agent question".to_string(),
        };
        let mut actions = div().flex().flex_wrap().gap_2().pt_1();
        let mut body = div().flex().flex_col().gap_2().w_full().min_w_0();
        match pi.kind.as_str() {
            "permission" => {
                for o in &pi.options {
                    let colors = if o.kind == "deny" {
                        (DANGER, 0xffffff)
                    } else {
                        (ACCENT, 0x000000)
                    };
                    let btn = self.answer_btn(
                        iid,
                        &o.option_id,
                        &o.label,
                        colors,
                        option_answer(&o.option_id),
                        cx,
                    );
                    actions = actions.child(btn);
                }
                if let Some(fa) = &pi.full_access {
                    let btn = self.answer_btn(
                        iid,
                        FULL_ACCESS_OPTION_ID,
                        &fa.label,
                        (AMBER, 0x000000),
                        option_answer(FULL_ACCESS_OPTION_ID),
                        cx,
                    );
                    actions = actions.child(btn);
                }
                if pi.free_text && pi.deny_option().is_some() {
                    body = body.child(self.interaction_input_row());
                    actions = actions.child(self.typed_answer_btn(pi, "Deny with feedback", cx));
                }
            }
            "userInput" => {
                if pi.questions.is_empty() {
                    for o in &pi.options {
                        let btn = self.answer_btn(
                            iid,
                            &o.option_id,
                            &o.label,
                            (ACCENT, 0x000000),
                            option_answer(&o.option_id),
                            cx,
                        );
                        actions = actions.child(btn);
                    }
                    if pi.free_text {
                        body = body.child(self.interaction_input_row());
                        actions = actions.child(self.typed_answer_btn(pi, "Send answer", cx));
                    }
                } else {
                    body = body.child(self.question_stepper(pi, cx));
                }
                actions = actions.child(self.answer_btn(
                    iid,
                    "decline",
                    "Decline",
                    (DANGER, 0xffffff),
                    decline_answer(),
                    cx,
                ));
            }
            _ => {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Review workspace hooks in the ZCode desktop app."),
                );
            }
        }
        div()
            .id(SharedString::from(format!("interaction-{iid}")))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .my_2()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(AMBER))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(AMBER))
                    .child(title),
            )
            .when(!pi.text.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .text_xs()
                        .text_color(rgb(TEXT))
                        .child(pi.text.clone()),
                )
            })
            .child(body)
            .child(actions)
            .into_any_element()
    }
}
