//! MCP servers inspection pane displaying connected servers, transports and tools.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts (zcodeMcpListResultSchema) and PARITY.md M7.

use crate::app::root::RootView;
use crate::shared::theme::{BORDER, CARD, DANGER, HOVER, MUTED, PANEL, SUCCESS, TEXT};
use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, ParentElement, Styled, div,
    prelude::*, px, rgb,
};

impl RootView {
    pub(crate) fn mcp_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let servers = self.state.read(cx).mcp_servers.clone();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(format!("MCP Servers ({})", servers.len())),
                    )
                    .child(
                        div()
                            .id("mcp-refresh")
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.state.update(cx, |state, cx| {
                                    state.fetch_mcp_servers(cx);
                                });
                            }))
                            .child("↻ Refresh"),
                    ),
            )
            .child(if servers.is_empty() {
                div()
                    .id("mcp-empty")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("No MCP servers found or still loading…")
            } else {
                div()
                    .id("mcp-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(servers.iter().map(|srv| {
                        let is_ok = srv.status == "connected";
                        let is_err = srv.status == "failed" || srv.error.is_some();
                        let badge_color = if is_err {
                            DANGER
                        } else if is_ok {
                            SUCCESS
                        } else {
                            MUTED
                        };

                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .p_2p5()
                            .bg(rgb(CARD))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .rounded_md()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(TEXT))
                                                    .child(srv.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .px_1p5()
                                                    .rounded_sm()
                                                    .bg(rgb(PANEL))
                                                    .text_color(rgb(MUTED))
                                                    .child(srv.transport.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(
                                                div()
                                                    .w(px(7.))
                                                    .h(px(7.))
                                                    .rounded_full()
                                                    .bg(rgb(badge_color)),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.5))
                                                    .text_color(rgb(badge_color))
                                                    .child(srv.status.clone()),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_size(px(10.5))
                                    .text_color(rgb(MUTED))
                                    .child(format!("{} tools available", srv.tool_count))
                                    .when_some(srv.updated_at.as_deref(), |el, updated| {
                                        el.child(
                                            div()
                                                .text_size(px(10.))
                                                .text_color(rgb(MUTED))
                                                .child(format!("Updated: {updated}")),
                                        )
                                    }),
                            )
                            .when_some(srv.error.as_deref(), |el, err| {
                                el.child(
                                    div()
                                        .p_1p5()
                                        .bg(rgb(0x271717))
                                        .border_1()
                                        .border_color(rgb(0x4a1d1d))
                                        .rounded_sm()
                                        .text_size(px(10.5))
                                        .text_color(rgb(DANGER))
                                        .child(err.to_string()),
                                )
                            })
                    }))
            })
            .into_any_element()
    }
}
