//! Plugin source cards (Personal Source marketplaces) and Restorable Builtin
//! rows for the plugin store. Split from plugin_cards.rs for the 400-line cap.

use crate::app::root::RootView;
use crate::shared::plugins::{AvailablePluginSummary, PluginMarketplaceSummary};
use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, TEXT};
use gpui::{
    AnyElement, Context, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::*, px, rgb,
};

impl RootView {
    pub(crate) fn render_marketplace_card(
        &self,
        m: &PluginMarketplaceSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m_id = m.id.clone();
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_2p5()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(m.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(format!(
                                "{} plugins • last updated: {}",
                                m.plugin_count,
                                m.last_updated.as_deref().unwrap_or("unknown")
                            )),
                    ),
            )
            .child({
                let btn_remove_id = format!("remove-{}", m_id);
                div()
                    .id(ElementId::Name(btn_remove_id.into()))
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(0x3f1d24))
                    .text_size(px(10.))
                    .text_color(rgb(DANGER))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|h| h.bg(rgb(0x5c1d24)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let id = m_id.clone();
                        this.state
                            .update(cx, |s, cx| s.remove_plugin_marketplace(&id, cx));
                    }))
                    .child("Remove")
            })
            .into_any_element()
    }

    pub(crate) fn render_restorable_builtin_row(
        &self,
        p: &AvailablePluginSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p_id = p.id.clone();
        let btn_restore_id = format!("restore-{}", p_id);
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_2()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(p.display_label().to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(p.description.clone().unwrap_or_default()),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(btn_restore_id.into()))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(rgb(ACCENT))
                    .text_size(px(10.5))
                    .text_color(rgb(0xffffff))
                    .cursor(CursorStyle::PointingHand)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let id = p_id.clone();
                        this.state
                            .update(cx, |s, cx| s.restore_builtin_plugin(&id, cx));
                    }))
                    .child("Restore"),
            )
            .into_any_element()
    }
}
