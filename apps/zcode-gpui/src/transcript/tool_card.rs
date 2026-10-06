//! Rendering of tool execution rows: a collapsed row with a human summary of
//! the call (desktop parity — the raw JSON is a last resort, not the default),
//! expandable to per-tool input views (Edit renders as a diff) and outputs.
use crate::shared::theme::ui_size;
use crate::shared::theme_colors::color as rgb;

use crate::conversation::model::format_preview;
use crate::shared::theme::{
    ACCENT, BORDER, CARD, CARD_HOVER, MONO_FONT, MUTED, PANEL, SELECTED, TEXT, TOOL, UI_FONT,
};
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px,
};
use serde_json::Value;

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
        let input: Option<Value> = serde_json::from_str(input_text).ok();

        div()
            .w_full()
            .flex()
            .flex_col()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg()
            .my_1p5()
            .child(
                div()
                    .id(ElementId::NamedInteger("toggle-tool".into(), r_id))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .hover(|h| h.bg(rgb(CARD_HOVER)))
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
                            .text_size(px(ui_size(9.)))
                            .text_color(rgb(MUTED))
                            .child(if expanded { "▼" } else { "▶" }),
                    )
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(ui_size(12.)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TOOL))
                            .child(label.to_string()),
                    )
                    .child(summary_label(label, input.as_ref()))
                    // Pushes the status chip to the trailing edge.
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(ui_size(10.)))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(PANEL))
                            .text_color(rgb(status_color))
                            .child(status.to_string()),
                    )
                    .when(label == "CreateWorkflow" || label == "Agent", |chip| {
                        chip.child(
                            div()
                                .id(ElementId::NamedInteger("subagent-badge".into(), r_id))
                                .px_1p5()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(CARD_HOVER))
                                .text_size(px(ui_size(9.5)))
                                .text_color(rgb(ACCENT))
                                .cursor_pointer()
                                .hover(|h| h.bg(rgb(SELECTED)))
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
            .when(expanded, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .px_3()
                        .pb_2p5()
                        .pt_1()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(input_view(label, input.as_ref(), input_text))
                        .when(!output_text.is_empty(), |out| {
                            out.child(if crate::shared::diff_view::looks_like_diff(output_text) {
                                crate::shared::diff_view::render_diff(output_text)
                            } else {
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(ui_size(11.)))
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

/// Trailing summary in the collapsed row; empty when the input is not JSON we
/// understand (the row then reads label + status only, never raw JSON).
fn summary_label(label: &str, input: Option<&Value>) -> AnyElement {
    let text = input.and_then(|v| tool_summary(label, v));
    let Some(text) = text else {
        return div().into_any_element();
    };
    div()
        .font_family(UI_FONT)
        .text_size(px(ui_size(12.)))
        .text_color(rgb(MUTED))
        .min_w_0()
        .overflow_hidden()
        .child(format_preview(&text, 80))
        .into_any_element()
}

/// One-line human summary per tool (desktop parity: primary param inline).
fn tool_summary(label: &str, v: &Value) -> Option<String> {
    let str_field = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    match label {
        "Bash" => str_field("command").map(|c| format!("$ {c}")),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => str_field("file_path")
            .or_else(|| str_field("notebook_path"))
            .or_else(|| str_field("path"))
            .map(short_path),
        "Grep" => Some(
            str_field("pattern")
                .map(|p| format!("\"{p}\""))
                .unwrap_or_default(),
        ),
        "Glob" => str_field("pattern"),
        "WebFetch" => str_field("url"),
        "WebSearch" => str_field("query"),
        "Task" | "Agent" => str_field("description").or_else(|| str_field("prompt")),
        _ => None,
    }
}

/// Last path segment, prefixed with its parent when that is informative
/// (`…/Test/Deadlock.c` for deep asset trees).
fn short_path(path: String) -> String {
    let mut parts: Vec<&str> = path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => path,
        1 => parts.remove(0).to_string(),
        _ => {
            let file = parts.pop().unwrap_or_default();
            let parent = parts.last().unwrap_or(&"");
            format!("{parent}/{file}")
        }
    }
}

/// Expanded input view: Bash shows the command, edits show a synthesized
/// +/- diff, anything else falls back to the raw wire text.
fn input_view(label: &str, input: Option<&Value>, raw: &str) -> AnyElement {
    if let Some(v) = input {
        match label {
            "Bash" => {
                if let Some(cmd) = v.get("command").and_then(Value::as_str) {
                    return mono_block(cmd);
                }
            }
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
                if let Some(diff) = edit_diff(v, label) {
                    return div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .font_family(UI_FONT)
                                .text_size(px(ui_size(11.5)))
                                .text_color(rgb(MUTED))
                                .child(
                                    v.get("file_path")
                                        .and_then(Value::as_str)
                                        .or_else(|| v.get("notebook_path").and_then(Value::as_str))
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                        )
                        .child(crate::shared::diff_view::render_diff(&diff))
                        .into_any_element();
                }
            }
            _ => {}
        }
    }
    mono_block(raw)
}

fn mono_block(text: &str) -> AnyElement {
    div()
        .font_family(MONO_FONT)
        .text_size(px(ui_size(11.)))
        .text_color(rgb(TEXT))
        .child(text.to_string())
        .into_any_element()
}

/// `-` old lines then `+` new lines — `render_diff` colors by line prefix.
fn edit_diff(v: &Value, label: &str) -> Option<String> {
    let pair = |old: &Value, new: &Value| -> Option<String> {
        let old = old.as_str()?;
        let new = new.as_str()?;
        let mut out = String::new();
        for line in old.lines() {
            out.push('-');
            out.push_str(line);
            out.push('\n');
        }
        for line in new.lines() {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        Some(out)
    };
    if label == "MultiEdit" {
        let edits = v.get("edits").and_then(Value::as_array)?;
        let mut out = String::new();
        for e in edits {
            // Skip partial entries instead of aborting the whole edit.
            if let (Some(old), Some(new)) = (e.get("old_string"), e.get("new_string"))
                && let Some(d) = pair(old, new)
            {
                out.push_str(&d);
            }
        }
        (!out.is_empty()).then_some(out)
    } else if label == "Write" {
        let content = v.get("content").and_then(Value::as_str)?;
        let mut out = String::new();
        for line in content.lines() {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        (!out.is_empty()).then_some(out)
    } else {
        pair(v.get("old_string")?, v.get("new_string")?)
    }
}
