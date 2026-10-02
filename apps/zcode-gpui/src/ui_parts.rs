//! Smaller pieces of the root layout: error banners, the composer intent
//! banner (editing a message / renaming a session) and the composer card.

use crate::composer::Composer;
use crate::msg_actions::ComposerIntent;
use crate::theme::{
    AMBER, BORDER, CARD, CARD_HOVER, DANGER, I_ARROW_UP, I_STOP, MUTED, PRIMARY, TEXT, icon,
};
use crate::ui::RootView;
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Div, Entity, Stateful, div, prelude::*, px, rgb,
};

fn banner() -> Div {
    div()
        .mx_3()
        .mt_2()
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(rgb(0x3d2323))
        .border_1()
        .border_color(rgb(DANGER))
}

impl RootView {
    /// Dismissable error banners (newest first) plus the backend turn error.
    pub(crate) fn error_banners(
        &self,
        errors: Vec<String>,
        phase_error: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = errors
            .into_iter()
            .rev()
            .enumerate()
            .map(|(i, msg)| {
                banner()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(rgb(DANGER))
                            .truncate()
                            .child(msg),
                    )
                    .child(
                        div()
                            .id(("err", i))
                            .px_1p5()
                            .cursor(CursorStyle::PointingHand)
                            .text_color(rgb(MUTED))
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.state.update(cx, |s, _cx| s.dismiss_error(i));
                                cx.notify();
                            }))
                            .child("✕"),
                    )
                    .into_any_element()
            })
            .collect();
        if let Some((code, message)) = phase_error {
            out.push(
                banner()
                    .text_size(px(12.))
                    .text_color(rgb(DANGER))
                    .child(format!("{code}: {message}"))
                    .into_any_element(),
            );
        }
        out
    }

    /// "Editing message" / "Renaming session" strip above the composer.
    pub(crate) fn intent_banner(
        &self,
        intent: &ComposerIntent,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let label = match intent {
            ComposerIntent::Send => return None,
            ComposerIntent::Edit { .. } => {
                "Editing message: Enter resends it and rewinds the conversation to this point"
            }
            ComposerIntent::Rename { .. } => "Renaming session: Enter saves the new title",
        };
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(AMBER))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(rgb(AMBER))
                        .child(label),
                )
                .child(
                    div()
                        .id("intent-cancel")
                        .px_1p5()
                        .text_size(px(11.5))
                        .text_color(rgb(MUTED))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.state.update(cx, |s, cx| s.cancel_intent(cx));
                        }))
                        .child("Cancel"),
                )
                .into_any_element(),
        )
    }
}

fn round_button(id: &'static str, glyph: char, bg: u32, fg: u32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(rgb(bg))
        .cursor(CursorStyle::PointingHand)
        .child(icon(glyph, 12., fg))
}

impl RootView {
    /// Desktop-style composer: one rounded card with the input on top and
    /// mode / model / thinking pickers plus the round send button below.
    pub(crate) fn composer_card(
        &mut self,
        composer: Entity<Composer>,
        running: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let send = round_button("send-btn", I_ARROW_UP, PRIMARY, 0x000000)
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| this.submit(cx)));
        let stop = round_button("stop-btn", I_STOP, CARD_HOVER, TEXT)
            .hover(|s| s.bg(rgb(BORDER)))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.update(cx, |s, cx| s.stop(cx));
            }));
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .pt_3()
            .pb_2()
            .rounded_xl()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(composer)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(self.mode_menu(cx))
                    .child(div().flex_1())
                    .child(self.composer_selectors(cx))
                    .when(running, |el| el.child(stop))
                    .child(send),
            )
    }
}
