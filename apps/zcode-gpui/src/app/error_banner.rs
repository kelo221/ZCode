//! Error banners above the transcript: queued errors (newest first, each
//! dismissable) and the backend turn error. The full message wraps and can
//! be selected; very long ones scroll inside the banner, and a Copy button
//! takes the whole text.

use crate::app::root::RootView;
use crate::shared::selectable::SelectableText;
use crate::shared::theme::{DANGER, MUTED, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Div, ElementId, ParentElement, Styled, div,
    prelude::*, px,
};

/// About six lines of 12px text before the banner scrolls.
const MAX_TEXT_HEIGHT: f32 = 96.;

fn banner() -> Div {
    div()
        .mx_3()
        .mt_2()
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(crate::shared::theme_colors::tinted(DANGER, 0.12))
        .border_1()
        .border_color(rgb(DANGER))
        .flex()
        .flex_row()
        .items_start()
        .gap_2()
}

fn message(id: ElementId, text: String) -> AnyElement {
    div()
        .id(id.clone())
        .flex_1()
        .min_w_0()
        .max_h(px(ui_size(MAX_TEXT_HEIGHT)))
        .overflow_y_scroll()
        .text_size(px(ui_size(12.)))
        .text_color(rgb(DANGER))
        .child(SelectableText::new(id, text))
        .into_any_element()
}

fn copy_button(id: ElementId, text: String) -> AnyElement {
    IconButton::new(id, IconName::Copy)
        .size(ControlSize::Sm)
        .variant(ButtonVariant::Ghost)
        .tooltip(crate::shared::i18n::label("Copy", "复制"))
        .on_click(move |_, _, cx: &mut gpui::App| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
        })
        .into_any_element()
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
                    .child(message(
                        ElementId::NamedInteger("err-text".into(), i as u64),
                        msg.clone(),
                    ))
                    .child(copy_button(
                        ElementId::NamedInteger("err-copy".into(), i as u64),
                        msg,
                    ))
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
        if let Some((code, msg)) = phase_error {
            let text = format!("{code}: {msg}");
            out.push(
                banner()
                    .child(message("phase-err-text".into(), text.clone()))
                    .child(copy_button("phase-err-copy".into(), text))
                    .into_any_element(),
            );
        }
        out
    }
}
