//! Composer Stop button while the agent works.

use crate::shared::theme::{ACCENT, BORDER, CARD_HOVER, I_STOP, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{CursorStyle, Div, ParentElement, Stateful, Styled, div, prelude::*, px};

/// Stop with a spinning ring, so a long tool call or hidden thinking still
/// shows that the agent is working.
pub(crate) fn working_stop_button() -> Stateful<Div> {
    use ely_gpui_component::{
        motion::{Spinner, SpinnerStyle},
        primitives::Tooltip,
        theme::IconSize,
    };
    div()
        .id("stop-btn")
        .relative()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .bg(rgb(CARD_HOVER))
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(BORDER)))
        .tooltip(Tooltip::text(crate::shared::i18n::label(
            "Agent is working · Stop",
            "智能体正在工作 · 停止",
        )))
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Spinner::new("stop-btn-spinner")
                        .style(SpinnerStyle::Ring)
                        .size(IconSize::Xl)
                        .color(rgb(ACCENT).into()),
                ),
        )
        .child(
            gpui::svg()
                .path(I_STOP.path())
                .size(px(10.))
                .text_color(rgb(TEXT)),
        )
}
