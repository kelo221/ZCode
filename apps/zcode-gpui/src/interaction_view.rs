//! Interaction cards: permission requests, agent questions (single/multi
//! question, multi-select), free-text answers taken from the message box.

use crate::interactions::{
    FULL_ACCESS_OPTION_ID, PendingInteraction, decline_answer, free_text_answer, option_answer,
    questions_answer, toggle_pick,
};
use crate::theme::{ACCENT, AMBER, BORDER, CARD, DANGER, MUTED, TEXT};
use crate::ui::RootView;
use gpui::{AnyElement, Context, Div, FontWeight, SharedString, Stateful, div, prelude::*, rgb};
use serde_json::Value;

fn action_btn(id: String, label: &str, bg: u32, fg: u32) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
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
    fn answer_btn(
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
                this.question_picks.remove(&iid);
                let answer = answer.clone();
                let iid = iid.clone();
                this.state
                    .update(cx, |app, cx| app.resolve_interaction(&iid, answer, cx));
            },
        ))
    }

    /// Free-text button: the message box text becomes the answer/feedback.
    fn free_text_btn(
        &self,
        iid: &str,
        label: &str,
        option_id: Option<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let iid = iid.to_string();
        action_btn(format!("ia-{iid}-text"), label, CARD, TEXT)
            .border_1()
            .border_color(rgb(BORDER))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let iid = iid.clone();
                let option_id = option_id.clone();
                this.state.update(cx, |app, cx| {
                    if let Some(text) = app.take_composer_answer(cx) {
                        let answer = free_text_answer(option_id.as_deref(), &text);
                        app.resolve_interaction(&iid, answer, cx);
                    }
                });
            }))
    }

    fn question_block(&self, pi: &PendingInteraction, cx: &mut Context<Self>) -> Div {
        let picks = self.question_picks.get(&pi.interaction_id);
        let mut block = div().flex().flex_col().gap_2();
        for (qi, q) in pi.questions.iter().enumerate() {
            let selected = picks.and_then(|p| p.get(&qi)).cloned().unwrap_or_default();
            let mut chips = div().flex().flex_wrap().gap_1();
            for (oi, (label, description)) in q.options.iter().enumerate() {
                let on = selected.contains(label);
                let (iid, label_c, multi) =
                    (pi.interaction_id.clone(), label.clone(), q.multi_select);
                chips = chips.child(
                    action_btn(
                        format!("iq-{iid}-{qi}-{oi}"),
                        label,
                        if on { ACCENT } else { CARD },
                        if on { 0x000000 } else { TEXT },
                    )
                    .border_1()
                    .border_color(rgb(BORDER))
                    .when(!description.is_empty(), |el| {
                        el.flex().flex_col().child(
                            div()
                                .font_weight(FontWeight::NORMAL)
                                .text_color(rgb(if on { 0x000000 } else { MUTED }))
                                .child(description.clone()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let picks = this.question_picks.entry(iid.clone()).or_default();
                        toggle_pick(picks.entry(qi).or_default(), &label_c, multi);
                        cx.notify();
                    })),
                );
            }
            block = block.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("{} · {}", q.header, q.question)),
                    )
                    .child(chips),
            );
        }
        block
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
        let mut body = div().flex().flex_col().gap_2();
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
                if pi.free_text
                    && let Some(deny) = pi.deny_option()
                {
                    let deny_id = Some(deny.option_id.clone());
                    actions = actions.child(self.free_text_btn(
                        iid,
                        "Deny with message as feedback",
                        deny_id,
                        cx,
                    ));
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
                } else {
                    body = body.child(self.question_block(pi, cx));
                    let iid_s = iid.to_string();
                    let questions = pi.questions.clone();
                    actions = actions.child(
                        action_btn(format!("ia-{iid}-submit"), "Submit", ACCENT, 0x000000)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let picks = this.question_picks.remove(&iid_s).unwrap_or_default();
                                let answer = questions_answer(&questions, &picks);
                                let iid = iid_s.clone();
                                this.state.update(cx, |app, cx| {
                                    app.resolve_interaction(&iid, answer, cx)
                                });
                            })),
                    );
                }
                if pi.free_text || !pi.questions.is_empty() {
                    actions = actions.child(self.free_text_btn(
                        iid,
                        "Answer with message text",
                        None,
                        cx,
                    ));
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
                el.child(div().text_xs().text_color(rgb(TEXT)).child(pi.text.clone()))
            })
            .child(body)
            .child(actions)
            .into_any_element()
    }
}
