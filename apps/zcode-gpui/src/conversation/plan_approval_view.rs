//! ExitPlanMode approval card (Desktop `V4InteractionDialogs.tsx` plan
//! branch): the plan rendered as markdown, Approve (text typed in the card
//! rides along as approval notes), a change request typed in the card, and
//! Decline.

use crate::app::root::RootView;
use crate::conversation::interaction_view::action_btn;
use crate::conversation::interactions::{PendingInteraction, decline_answer};
use crate::shared::markdown::render_markdown;
use crate::shared::theme::{ACCENT, BORDER, CODE_BG, DANGER, MUTED};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Context, ElementId, FontWeight, div, prelude::*, px};

impl RootView {
    pub(crate) fn plan_approval_card(
        &self,
        pi: &PendingInteraction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let iid = pi.interaction_id.as_str();
        let plan = pi.plan.as_deref().unwrap_or("").trim();
        // Seeded from the interaction id so markdown link ids never collide with rows.
        let id_seed = iid
            .bytes()
            .fold(0x9e37_u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
            | (1 << 40);
        let body = if plan.is_empty() {
            div()
                .text_sm()
                .text_color(rgb(MUTED))
                .child("The agent did not include a plan.")
                .into_any_element()
        } else {
            div()
                .id(ElementId::Name(format!("plan-body-{iid}").into()))
                .w_full()
                .min_w_0()
                .max_h(px(480.))
                .overflow_y_scroll()
                .p_3()
                .rounded_md()
                .bg(rgb(CODE_BG))
                .border_1()
                .border_color(rgb(BORDER))
                .child(render_markdown(plan, id_seed, false))
                .into_any_element()
        };
        let approve_pi = pi.clone();
        let approve = action_btn(
            format!("ia-{iid}-approve"),
            "Approve and start",
            ACCENT,
            0x000000,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.approve_plan(&approve_pi, cx);
        }));
        let feedback = self.typed_answer_btn(pi, "Request changes", cx);
        let decline = self.answer_btn(
            iid,
            "decline",
            "Decline",
            (DANGER, 0xffffff),
            decline_answer(),
            cx,
        );

        div()
            .id(ElementId::Name(format!("interaction-{iid}").into()))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .my_2()
            .rounded_md()
            .bg(rgb(crate::shared::theme::CARD))
            .border_1()
            .border_color(rgb(ACCENT))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(ACCENT))
                    .child("Plan ready for review"),
            )
            .when(!pi.text.is_empty(), |el| {
                el.child(
                    div()
                        .w_full()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(pi.text.clone()),
                )
            })
            .child(body)
            .child(self.interaction_input_row())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .pt_1()
                    .child(approve)
                    .child(feedback)
                    .child(decline),
            )
            .into_any_element()
    }
}
