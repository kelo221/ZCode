//! MCP servers inspection pane displaying connected servers, transports and tools.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts (zcodeMcpListResultSchema) and PARITY.md M7.

use crate::app::root::RootView;
use crate::shared::theme::ui_size;
use crate::shared::theme::{BORDER, CARD, DANGER, MUTED, PANEL, SUCCESS, TEXT};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

impl RootView {
    pub(crate) fn mcp_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.ensure_inspection(crate::app::inspection_view::InspectionKind::Mcp, cx);
        let feedback =
            self.inspection_feedback(crate::app::inspection_view::InspectionKind::Mcp, cx);
        let servers = self
            .state
            .read(cx)
            .active_inspection()
            .and_then(|i| i.mcp.value.clone())
            .unwrap_or_default();

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
                            .text_size(px(ui_size(12.)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(format!(
                                "{} ({})",
                                crate::shared::i18n::label("MCP Servers", "MCP 服务器"),
                                servers.len()
                            )),
                    )
                    .child({
                        let refresh = div().child(
                            Button::new(
                                "mcp-refresh",
                                crate::shared::i18n::label("Refresh", "刷新"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state
                                    .update(cx, |state, cx| state.fetch_mcp_servers(cx));
                            })),
                        );
                        #[cfg(test)]
                        let refresh = crate::app::test_support::track_children(
                            refresh,
                            vec!["mcp-refresh".into()],
                        );
                        refresh
                    }),
            )
            .child(feedback)
            .child(if servers.is_empty() {
                div()
                    .id("mcp-empty")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(crate::shared::i18n::label(
                        "No MCP servers to display",
                        "没有可显示的 MCP 服务器",
                    ))
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
                                                    .text_size(px(ui_size(12.)))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgb(TEXT))
                                                    .child(srv.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(ui_size(10.)))
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
                                                    .text_size(px(ui_size(10.5)))
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
                                    .text_size(px(ui_size(10.5)))
                                    .text_color(rgb(MUTED))
                                    .child(format!(
                                        "{} {}",
                                        srv.tool_count,
                                        crate::shared::i18n::label("tools available", "个可用工具")
                                    ))
                                    .when_some(srv.updated_at.as_deref(), |el, updated| {
                                        el.child(
                                            div()
                                                .text_size(px(ui_size(10.)))
                                                .text_color(rgb(MUTED))
                                                .child(format!(
                                                    "{}: {updated}",
                                                    crate::shared::i18n::label(
                                                        "Updated",
                                                        "更新时间"
                                                    )
                                                )),
                                        )
                                    }),
                            )
                            .when_some(srv.error.as_deref(), |el, err| {
                                el.child(
                                    div()
                                        .p_1p5()
                                        .bg(rgb(PANEL))
                                        .border_1()
                                        .border_color(rgb(DANGER))
                                        .rounded_sm()
                                        .text_size(px(ui_size(10.5)))
                                        .text_color(rgb(DANGER))
                                        .child(err.to_string()),
                                )
                            })
                            .children(self.mcp_authorization_button(&srv.name, cx))
                    }))
            })
            .into_any_element()
    }
}
