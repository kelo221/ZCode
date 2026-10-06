use crate::shared::theme::{BORDER, CARD, HOVER, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{CursorStyle, ParentElement, Stateful, Styled, div, prelude::*, px};

pub(crate) fn side_btn(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded_sm()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(ui_size(11.)))
        .text_color(rgb(TEXT))
        .cursor(CursorStyle::PointingHand)
        .hover(|h| h.bg(rgb(HOVER)))
        .child(label)
        .on_click(on_click)
}
