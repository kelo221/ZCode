//! Sidebar section header ("Tasks", "Projects"): a muted label with a "+"
//! action on the right.

use crate::shared::theme::{HOVER, I_ADD, MUTED, icon, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{App, ClickEvent, CursorStyle, Div, SharedString, Window, div, prelude::*, px};

pub(crate) fn section_header(
    action_id: &'static str,
    label: SharedString,
    on_add: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .pl_4()
        .pr_3()
        .pt_3()
        .pb_1()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(ui_size(12.)))
                .text_color(rgb(MUTED))
                .child(label),
        )
        .child(
            div()
                .id(action_id)
                .cursor(CursorStyle::PointingHand)
                .hover(|s| s.bg(rgb(HOVER)))
                .rounded_md()
                .p_1()
                .on_click(on_add)
                .child(icon(I_ADD, 14., MUTED)),
        )
}
