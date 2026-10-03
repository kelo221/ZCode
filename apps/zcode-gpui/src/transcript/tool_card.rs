//! Rendering of tool execution rows with expandable inputs, outputs, and diff views.

use crate::shared::theme::{ACCENT, BORDER, CARD, MUTED, PANEL, TEXT, TOOL};
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb,
};

impl crate::app::root::RootView {
    pub(crate) fn render_tool_row(
        &self,
        r_id: u64,
        label: &str,
        status: &str,
        input_text: &str,
        output_text: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_tools.contains(&r_id);
        let is_running = status == "running" || status == "inputStreaming";
        let is_error = status == "error";

        let status_color = if is_error {
            crate::shared::theme::DANGER
        } else if is_running {
            ACCENT
        } else {
            MUTED
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_md()
            .my_1()
            .child(
                div()
                    .id(ElementId::NamedInteger("toggle-tool".into(), r_id))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2p5()
                    .py_1p5()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if this.expanded_tools.contains(&r_id) {
                            this.expanded_tools.remove(&r_id);
                        } else {
                            this.expanded_tools.insert(r_id);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .child(if expanded { "▼" } else { "▶" }),
                            )
                            .child(
                                div()
                                    .font_family("Consolas")
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TOOL))
                                    .child(label.to_string()),
                            )
                            .when(label == "CreateWorkflow" || label == "Agent", |chip| {
                                chip.child(
                                    div()
                                        .id(ElementId::NamedInteger("subagent-badge".into(), r_id))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(0x1e293b))
                                        .text_size(px(9.5))
                                        .text_color(rgb(ACCENT))
                                        .cursor_pointer()
                                        .hover(|h| h.bg(rgb(0x334155)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.dock_open = true;
                                            this.dock_tab = crate::app::dock::DockTab::Workflows;
                                            cx.notify();
                                        }))
                                        .child("⚡ Subagents"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .px_1p5()
                            .rounded_sm()
                            .bg(rgb(PANEL))
                            .text_color(rgb(status_color))
                            .child(status.to_string()),
                    ),
            )
            .when(expanded, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_2p5()
                        .pb_2()
                        .pt_1()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .when(!input_text.is_empty(), |inp| {
                            inp.child(
                                div()
                                    .font_family("Consolas")
                                    .text_size(px(11.))
                                    .text_color(rgb(TEXT))
                                    .child(input_text.to_string()),
                            )
                        })
                        .when(!output_text.is_empty(), |out| {
                            out.child(if crate::shared::diff_view::looks_like_diff(output_text) {
                                crate::shared::diff_view::render_diff(output_text)
                            } else {
                                div()
                                    .font_family("Consolas")
                                    .text_size(px(11.))
                                    .text_color(rgb(MUTED))
                                    .child(output_text.to_string())
                                    .into_any_element()
                            })
                        }),
                )
            })
            .into_any_element()
    }
}
