//! Agents dock section: list of currently running subagents with status,
//! elapsed duration, summary preview, and Stop/Open controls.

use crate::app::root::RootView;
use crate::shared::theme::ui_size;
use crate::shared::theme::{
    ACCENT, BORDER, CARD, CARD_HOVER, DANGER, HOVER, MUTED, PANEL, SUCCESS, TEXT, WARNING,
};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

/// Format title or type fallback for an agent item.
pub fn format_agent_title<'a>(title: &'a str, subagent_type: &'a str) -> &'a str {
    let t = title.trim();
    if !t.is_empty() { t } else { subagent_type }
}

/// Truncate long summary text with an ellipsis.
pub fn truncate_summary(summary: Option<&str>, max_chars: usize) -> Option<String> {
    let text = summary?.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > max_chars {
        let prefix: String = text.chars().take(max_chars).collect();
        Some(format!("{prefix}..."))
    } else {
        Some(text.to_string())
    }
}

/// Format ended subagent count summary label.
pub fn format_ended_summary(ended_total: u64) -> Option<String> {
    if ended_total == 0 {
        None
    } else {
        Some(format!(
            "{ended_total} ended subagent{}",
            if ended_total == 1 { "" } else { "s" }
        ))
    }
}

impl RootView {
    /// Render the collapsible Agents section for the review dock pane.
    pub(crate) fn render_agents_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let ended = self.render_ended_agents(cx);
        let state = self.state.read(cx);
        let conv = state.active_conversation();
        let mut running = conv.map(|c| c.running_subagents()).unwrap_or_default();
        running.sort_by(|a, b| {
            a.started_at
                .cmp(&b.started_at)
                .then(a.child_session_id.cmp(&b.child_session_id))
        });
        let owner = state.active_ws_key().zip(state.active.clone());
        let running_count = running.len();
        let ended_count = conv
            .and_then(|c| c.subagents.as_ref())
            .map(|s| s.ended_total)
            .unwrap_or(0);

        let expanded = self.agents_expanded;
        let counter_color = if running_count > 0 { ACCENT } else { MUTED };

        let header = div()
            .id("agents-section-header")
            .h(px(32.))
            .min_w_0()
            .flex()
            .items_center()
            .gap_1p5()
            .px_3()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(HOVER)))
            .rounded_sm()
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.agents_expanded = !this.agents_expanded;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(TEXT))
                    .child("Agents"),
            )
            .child(
                div()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child(if expanded { "▾" } else { "▸" }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(counter_color))
                    .child(format!("{running_count}")),
            );

        let content = if running.is_empty() {
            let msg = if ended_count > 0 {
                format!("No running subagents ({ended_count} ended)")
            } else {
                "No running subagents".to_string()
            };
            div()
                .px_2()
                .py_2()
                .text_size(px(ui_size(11.5)))
                .text_color(rgb(MUTED))
                .child(msg)
                .into_any_element()
        } else {
            let mut list = div()
                .id("agents-running-list")
                .max_h(px(240.))
                .min_h_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1p5()
                .px_2()
                .pb_1p5();

            for (idx, agent) in running.iter().enumerate() {
                let dot_color = match agent.status.as_str() {
                    "running" => ACCENT,
                    "blocked" => WARNING,
                    "failed" | "error" => DANGER,
                    "success" | "completed" => SUCCESS,
                    _ => MUTED,
                };

                let title = format_agent_title(&agent.title, &agent.subagent_type);
                let elapsed = agent.started_at.map(|t| {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(t);
                    let diff = now_ms.saturating_sub(t);
                    crate::conversation::turn_meta::format_duration(diff)
                });

                let stop_button = agent
                    .work_id
                    .clone()
                    .filter(|_| agent.cancellable)
                    .and_then(|wid| {
                        self.activity_cancel_button(format!("dock-stop-subagent-{idx}"), wid, cx)
                    });

                let open_button = if !agent.child_session_id.is_empty() {
                    let sid_clone = agent.child_session_id.clone();
                    let owner = owner.clone();
                    let element = div().child(
                        div()
                            .id(ElementId::NamedInteger(
                                "dock-open-subagent".into(),
                                idx as u64,
                            ))
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_size(px(ui_size(10.5)))
                            .text_color(rgb(ACCENT))
                            .hover(|h| h.bg(rgb(CARD_HOVER)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some((workspace, parent)) = &owner {
                                    this.open_child_conversation(
                                        workspace, parent, &sid_clone, window, cx,
                                    );
                                }
                            }))
                            .child(crate::shared::i18n::label("Open", "打开")),
                    );
                    #[cfg(test)]
                    let element = crate::app::test_support::track_children(
                        element,
                        vec![format!("agent-open-{}", agent.child_session_id)],
                    );
                    Some(element)
                } else {
                    None
                };

                let summary_node = truncate_summary(agent.summary.as_deref(), 160).map(|txt| {
                    div()
                        .mt_1()
                        .text_size(px(ui_size(11.)))
                        .text_color(rgb(MUTED))
                        .child(txt)
                });

                let item = div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded_md()
                    .p_1p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .w(px(6.5))
                                            .h(px(6.5))
                                            .rounded_full()
                                            .bg(rgb(dot_color)),
                                    )
                                    .child(
                                        div()
                                            .font_family(crate::shared::theme::MONO_FONT)
                                            .text_size(px(ui_size(11.5)))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT))
                                            .child(title.to_string()),
                                    )
                                    .when_some(elapsed, |el, dur| {
                                        el.child(
                                            div()
                                                .text_size(px(ui_size(10.5)))
                                                .text_color(rgb(MUTED))
                                                .child(format!("({dur})")),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .children(stop_button)
                                    .children(open_button),
                            ),
                    )
                    .children(summary_node);

                list = list.child(item);
            }

            if let Some(ended_msg) = format_ended_summary(ended_count) {
                list = list.child(
                    div()
                        .px_2()
                        .pt_1()
                        .text_size(px(ui_size(10.5)))
                        .text_color(rgb(MUTED))
                        .child(ended_msg),
                );
            }

            list.into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(BORDER))
            .pt_1()
            .child(header)
            .when(expanded, |el| el.child(content))
            .children(ended)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "agents_section_tests.rs"]
mod tests;
