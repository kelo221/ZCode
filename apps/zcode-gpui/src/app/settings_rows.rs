//! Desktop `SettingsGroupCard` / `SettingsRow`: a bordered card of rows, each
//! with its label and description on the left and the control on the right.

use crate::shared::theme::{active_theme, ui_size};
use gpui::{AnyElement, Div, SharedString, div, prelude::*, px};

/// Control column width: Desktop `192px`, or `280px` for wide controls.
pub(crate) enum ControlWidth {
    Default,
    Wide,
}

pub(crate) fn settings_card(rows: impl IntoIterator<Item = AnyElement>) -> Div {
    let theme = active_theme();
    let mut card = div()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .flex()
        .flex_col();
    for (index, row) in rows.into_iter().enumerate() {
        card = card.child(
            div()
                .when(index > 0, |el| el.border_t_1().border_color(theme.border))
                .child(row),
        );
    }
    card
}

pub(crate) fn settings_row(
    label: impl Into<SharedString>,
    description: Option<SharedString>,
    control: impl IntoElement,
    width: ControlWidth,
) -> AnyElement {
    let theme = active_theme();
    div()
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .text_size(px(ui_size(14.)))
                .child(
                    div()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(label.into()),
                )
                .children(description.map(|text| div().text_color(theme.subtle).child(text))),
        )
        .child(
            div()
                .flex_none()
                .w(px(match width {
                    ControlWidth::Default => 192.,
                    ControlWidth::Wide => 280.,
                }))
                .flex()
                .justify_end()
                .child(control),
        )
        .into_any_element()
}
