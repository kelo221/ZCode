//! Usage stats pane displaying token metrics and daily timeline.
//!
//! Spec source: packages/shared/src/usage-stats.ts and PARITY.md M7.

use crate::app::root::RootView;
use crate::shared::theme::{ACCENT, BORDER, CARD, HOVER, MUTED, PANEL, TEXT};
use gpui::{
    AnyElement, Context, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::*, px, rgb,
};

impl RootView {
    pub(crate) fn usage_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let usage = self.state.read(cx).usage_stats.clone();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .min_h_0()
            .child(self.render_usage_header(cx))
            .child(match usage {
                Some(snap) => div()
                    .id("usage-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .gap_3()
                    .flex()
                    .flex_col()
                    .child(self.render_usage_summary(&snap))
                    .child(self.render_usage_daily(&snap)),
                None => div()
                    .id("usage-loading")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("Loading usage stats…"),
            })
            .into_any_element()
    }

    fn render_usage_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let current_range = self
            .state
            .read(cx)
            .usage_stats
            .as_ref()
            .map(|u| u.range.clone())
            .unwrap_or_else(|| "7d".to_string());

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
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(["7d", "30d", "all"].iter().map(|&range| {
                        let selected = range == current_range;
                        div()
                            .id(ElementId::NamedInteger(
                                "usage-range".into(),
                                match range {
                                    "7d" => 7,
                                    "30d" => 30,
                                    _ => 0,
                                },
                            ))
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .text_size(px(11.))
                            .cursor(CursorStyle::PointingHand)
                            .when(selected, |el| el.bg(rgb(CARD)).text_color(rgb(TEXT)))
                            .when(!selected, |el| {
                                el.text_color(rgb(MUTED)).hover(|h| h.bg(rgb(HOVER)))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.state.update(cx, |state, cx| {
                                    state.fetch_usage_stats(range, cx);
                                });
                            }))
                            .child(range)
                    })),
            )
            .child(
                div()
                    .id("usage-refresh")
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|h| h.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let r = current_range.clone();
                        this.state.update(cx, |state, cx| {
                            state.fetch_usage_stats(&r, cx);
                        });
                    }))
                    .child("↻ Refresh"),
            )
            .into_any_element()
    }

    fn render_usage_summary(
        &self,
        snap: &crate::shared::usage_stats::AppUsageSnapshot,
    ) -> AnyElement {
        let sum = &snap.summary;
        let cache_pct = (sum.cache_hit_rate * 100.0).round() as u64;

        div()
            .flex()
            .flex_col()
            .gap_2()
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
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child("Tokens & Turns"),
                    )
                    .when_some(sum.favorite_model.as_deref(), |el, model| {
                        el.child(
                            div()
                                .text_size(px(10.5))
                                .text_color(rgb(ACCENT))
                                .child(format!("★ {model}")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(self.stat_box("Total Tokens", &format_count(sum.total_tokens)))
                    .child(self.stat_box("Input", &format_count(sum.input_tokens)))
                    .child(self.stat_box("Output", &format_count(sum.output_tokens)))
                    .child(self.stat_box("Reasoning", &format_count(sum.reasoning_tokens))),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .pt_1()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(self.stat_box("Cache Hit", &format!("{cache_pct}%")))
                    .child(self.stat_box("Sessions", &sum.total_sessions.to_string()))
                    .child(self.stat_box("Turns", &sum.total_turns.to_string()))
                    .child(self.stat_box("Tool Calls", &sum.tool_call_count.to_string())),
            )
            .into_any_element()
    }

    fn render_usage_daily(
        &self,
        snap: &crate::shared::usage_stats::AppUsageSnapshot,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("Daily Timeline"),
            )
            .children(snap.daily.iter().rev().take(14).map(|day| {
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT))
                            .child(day.date.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(MUTED))
                                    .child(format!("{} turns", day.turn_count)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(MUTED))
                                    .child(format!("{} tools", day.tool_call_count)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(ACCENT))
                                    .child(format!("{} tokens", format_count(day.total_tokens))),
                            ),
                    )
            }))
            .into_any_element()
    }

    fn stat_box(&self, label: &'static str, value: &str) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(label))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(TEXT))
                    .child(value.to_string()),
            )
            .into_any_element()
    }
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
