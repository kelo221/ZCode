//! Rendering of tool execution rows: a collapsed row with a human summary of
//! the call (desktop parity — the raw JSON is a last resort, not the default),
//! expandable to per-tool input views (Edit renders as a diff) and outputs.
use crate::shared::theme::ui_size;
use crate::shared::theme_colors::color as rgb;

use crate::conversation::model::format_preview;
use crate::shared::theme::{
    BG, BORDER, DANGER, I_CHEVRON_DOWN, I_CHEVRON_RIGHT, MONO_FONT, MUTED, SUBTLEST, TAG, TEXT,
    UI_FONT, icon,
};
use crate::shared::theme_colors::tinted;
use ely_gpui_component::primitives::IconName;
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px,
};
use serde_json::Value;

const TOOL_GROUP: &str = "tool-summary";

/// Desktop renderer icon per tool kind (`ToolCallBlocks/renderers/*`).
fn tool_icon(label: &str) -> IconName {
    match label {
        "Read" | "Grep" | "Glob" | "LS" => IconName::Search,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => IconName::Pencil,
        "Bash" => IconName::Terminal,
        "WebFetch" | "WebSearch" => IconName::Globe,
        "Task" | "Agent" | "CreateWorkflow" => IconName::Bot,
        "TodoWrite" | "TodoRead" => IconName::ListTodo,
        _ => IconName::Wrench,
    }
}

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
        let input: Option<Value> = serde_json::from_str(input_text).ok();

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(ElementId::NamedInteger("toggle-tool".into(), r_id))
                    .group(TOOL_GROUP)
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(ui_size(14.)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.expanded_tools.remove(&r_id) {
                            this.expanded_tools.insert(r_id);
                        }
                        cx.notify();
                    }))
                    .child(icon(tool_icon(label), 16., MUTED))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if is_running { MUTED } else { SUBTLEST }))
                            .child(label.to_string()),
                    )
                    .child(summary_label(label, input.as_ref()))
                    .when(is_error, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_color(rgb(DANGER))
                                .child(crate::shared::i18n::label("Failed", "失败")),
                        )
                    })
                    .when(label == "CreateWorkflow" || label == "Agent", |chip| {
                        chip.child(
                            div()
                                .id(ElementId::NamedInteger("subagent-badge".into(), r_id))
                                .flex_none()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .bg(tinted(TAG, 0.5))
                                .text_size(px(ui_size(12.)))
                                .text_color(rgb(MUTED))
                                .cursor_pointer()
                                .hover(|h| h.text_color(rgb(TEXT)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.dock_open = true;
                                    this.dock_tab = crate::app::dock::DockTab::Workflows;
                                    cx.notify();
                                }))
                                .child("Subagents"),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .when(!expanded, |el| {
                                el.opacity(0.).group_hover(TOOL_GROUP, |s| s.opacity(1.))
                            })
                            .child(icon(
                                if expanded {
                                    I_CHEVRON_DOWN
                                } else {
                                    I_CHEVRON_RIGHT
                                },
                                16.,
                                SUBTLEST,
                            )),
                    ),
            )
            .when(expanded, |el| {
                el.child(
                    div().pt_2().child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(BG))
                            .child(input_view(label, input.as_ref(), input_text))
                            .when(!output_text.is_empty(), |out| {
                                out.child(
                                    if crate::shared::diff_view::looks_like_diff(output_text) {
                                        crate::shared::diff_view::render_diff(output_text)
                                    } else {
                                        div()
                                            .font_family(MONO_FONT)
                                            .text_size(px(ui_size(12.)))
                                            .text_color(rgb(MUTED))
                                            // gpui 不绘制制表符，`cat -n` 风格的行号会与正文粘连；展开为空格。
                                            .child(output_text.replace('\t', "    "))
                                            .into_any_element()
                                    },
                                )
                            }),
                    ),
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
        .text_color(rgb(MUTED))
        .min_w_0()
        .truncate()
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
