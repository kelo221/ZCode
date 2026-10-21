//! AskUserQuestion stepper (Desktop `ElicitationDialog.tsx`): one question at
//! a time with a header badge and "n / N" counter, full-width wrapping option
//! rows, Back, and a primary button that reads Continue until the last
//! question and Submit on it.

use crate::app::root::RootView;
use crate::conversation::interaction_view::action_btn;
use crate::conversation::interactions::{PendingInteraction, Question, step_question, toggle_pick};
use crate::shared::theme::{ACCENT, BORDER, CARD, CARD_HOVER, MUTED, TAG, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{Context, Div, FontWeight, SharedString, div, prelude::*};

impl RootView {
    fn question_option_rows(
        &self,
        iid: &str,
        qi: usize,
        q: &Question,
        selected: &[String],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut rows = div().flex().flex_col().gap_1().w_full().min_w_0();
        // Typed text replaces a single-select pick (Desktop updateCustomAnswer).
        let typed = !q.multi_select && !self.typed_text(iid, qi, cx).is_empty();
        for (oi, opt) in q.options.iter().enumerate() {
            let on = !typed && selected.contains(&opt.value);
            let (iid_c, value, multi) = (iid.to_string(), opt.value.clone(), q.multi_select);
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("iq-{iid}-{qi}-{oi}")))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if on { ACCENT } else { BORDER }))
                    .bg(rgb(if on { CARD_HOVER } else { CARD }))
                    .hover(|s| s.bg(rgb(CARD_HOVER)))
                    .cursor_pointer()
                    .child(
                        div()
                            .w_full()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if on { ACCENT } else { TEXT }))
                            .child(format!(
                                "{} {}",
                                match (multi, on) {
                                    (true, true) => "☑",
                                    (true, false) => "☐",
                                    (false, true) => "●",
                                    (false, false) => "○",
                                },
                                opt.label
                            )),
                    )
                    .when(!opt.description.is_empty(), |el| {
                        el.child(
                            div()
                                .w_full()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(opt.description.clone()),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let picks = this.question_picks.entry(iid_c.clone()).or_default();
                        toggle_pick(picks.entry(qi).or_default(), &value, multi);
                        if !multi {
                            this.clear_typed_text(&iid_c, qi, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        rows
    }

    /// The current question of `pi` plus its Back / Continue-or-Submit row.
    pub(crate) fn question_stepper(&self, pi: &PendingInteraction, cx: &mut Context<Self>) -> Div {
        let iid = pi.interaction_id.clone();
        let total = pi.questions.len();
        let qi = self
            .question_steps
            .get(&iid)
            .copied()
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let q = &pi.questions[qi];
        let selected = self
            .question_picks
            .get(&iid)
            .and_then(|p| p.get(&qi))
            .cloned()
            .unwrap_or_default();
        let last = qi + 1 == total;

        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .min_w_0()
            .when(!q.header.is_empty(), |el| {
                el.child(
                    div()
                        .flex_none()
                        .px_1p5()
                        .rounded_sm()
                        .bg(rgb(TAG))
                        .text_xs()
                        .text_color(rgb(TEXT))
                        .child(q.header.clone()),
                )
            })
            .when(total > 1, |el| {
                el.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!("{} / {}", qi + 1, total)),
                )
            })
            .when(q.multi_select, |el| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Select all that apply"),
                )
            });

        let mut nav = div().flex().flex_wrap().gap_2();
        if qi > 0 {
            let iid_b = iid.clone();
            nav = nav.child(
                action_btn(format!("ia-{iid}-back"), "Back", CARD, TEXT)
                    .border_1()
                    .border_color(rgb(BORDER))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let step = this.question_steps.entry(iid_b.clone()).or_default();
                        *step = step_question(*step, false, total);
                        cx.notify();
                    })),
            );
        }
        let primary = if last { "Submit" } else { "Continue" };
        let pi_submit = pi.clone();
        nav = nav.child(
            action_btn(format!("ia-{iid}-next"), primary, ACCENT, 0x000000).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    if !last {
                        let step = this.question_steps.entry(iid.clone()).or_default();
                        *step = step_question(*step, true, total);
                        cx.notify();
                        return;
                    }
                    this.submit_questions(&pi_submit, cx);
                },
            )),
        );

        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(header)
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(q.question.clone()),
            )
            .child(self.question_option_rows(&pi.interaction_id, qi, q, &selected, cx))
            .child(self.interaction_input_row())
            .child(nav)
    }
}
