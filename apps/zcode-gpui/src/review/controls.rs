use crate::app::root::RootView;
use crate::shared::theme::{BORDER, CARD, DANGER, HOVER, SUCCESS, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, CursorStyle, ParentElement, Stateful, Styled, div, prelude::*, px};

impl RootView {
    /// Outcome of the last git action (moved from review/pane.rs for the
    /// 400-line cap).
    pub(crate) fn notice_banner(&self) -> Option<AnyElement> {
        let (ok, msg) = self.git.notice.as_ref()?;
        Some(
            div()
                .px_2()
                .py_1()
                .text_size(px(ui_size(11.)))
                .text_color(rgb(if *ok { SUCCESS } else { DANGER }))
                .child(msg.clone())
                .into_any_element(),
        )
    }
}

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
