//! Smaller pieces of the root layout: error banners, the composer intent
//! banner (editing a message / renaming a session) and the send/stop buttons.

use crate::msg_actions::ComposerIntent;
use crate::theme::{ACCENT, AMBER, BORDER, CARD, DANGER, MUTED, TEXT};
use crate::ui::RootView;
use gpui::{AnyElement, ClickEvent, Context, CursorStyle, Div, Stateful, div, prelude::*, px, rgb};

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

pub(crate) fn send_button(intent: &ComposerIntent, cx: &mut Context<RootView>) -> Stateful<Div> {
    let label = match intent {
        ComposerIntent::Send => "Send",
        ComposerIntent::Edit { .. } => "Resend",
        ComposerIntent::Rename { .. } => "Save",
    };
    div()
        .id("send-btn")
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(0x1f3d3a))
        .text_size(px(12.5))
        .text_color(rgb(ACCENT))
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x2a524d)))
        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
            this.submit(cx);
        }))
        .child(label)
}

pub(crate) fn stop_button(cx: &mut Context<RootView>) -> Stateful<Div> {
    div()
        .id("stop-btn")
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(0x3d2323))
        .text_size(px(12.5))
        .text_color(rgb(DANGER))
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x522c2c)))
        .border_color(rgb(BORDER))
        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
            this.state.update(cx, |s, cx| s.stop(cx));
        }))
        .child("Stop")
}
