//! Turn metadata, execution duration, file changes, and plan progress checklist.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (rows.ts:56-97, snapshot.ts:405-449).

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};
use serde_json::Value;

use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, SUCCESS, TEXT};

#[derive(Clone, Debug, PartialEq)]
pub struct FileChanges {
    pub files: u32,
    pub additions: u32,
    pub deletions: u32,
    pub state: String,
}

impl FileChanges {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            files: v.get("files").and_then(Value::as_u64).unwrap_or(0) as u32,
            additions: v.get("additions").and_then(Value::as_u64).unwrap_or(0) as u32,
            deletions: v.get("deletions").and_then(Value::as_u64).unwrap_or(0) as u32,
            state: v
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("active")
                .to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanItem {
    pub id: String,
    pub content: String,
    pub status: String,
}

impl PlanItem {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            id: v
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            content: v
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            status: v
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanState {
    pub items: Vec<PlanItem>,
    pub updated_at: i64,
}

impl PlanState {
    pub fn from_value(v: &Value) -> Option<Self> {
        let items = v
            .get("items")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(PlanItem::from_value)
            .collect();
        let updated_at = v.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
        Some(Self { items, updated_at })
    }

    pub fn completed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|it| it.status == "completed")
            .count()
    }
}

pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{secs}s")
    } else {
        let mins = secs / 60;
        let rem = secs % 60;
        format!("{mins}m {rem}s")
    }
}

/// Render a turn header row with duration, state, file changes summary, and Undo button.
pub fn render_turn_header(
    (row_id, entity_id): (u64, &str),
    state: &str,
    active_ms: Option<u64>,
    file_changes: Option<&FileChanges>,
    can_rewind: bool,
    confirming: bool,
    cx: &mut Context<crate::app::root::RootView>,
) -> AnyElement {
    let failed = state == "failed" || state == "completedInterrupted";
    let is_running = state == "running";

    let mut info_text = Vec::new();
    if is_running {
        info_text.push("Running…".to_string());
    } else if let Some(ms) = active_ms
        && ms > 0
    {
        info_text.push(format!("Worked for {}", format_duration(ms)));
    }

    let ent_id = entity_id.to_string();

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .my_2()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_1().h(px(1.)).bg(rgb(BORDER)))
                .when(!info_text.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(info_text.join(" · ")),
                    )
                })
                .when(failed, |el| {
                    el.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(DANGER))
                            .child(state.to_string()),
                    )
                })
                .child(div().flex_1().h(px(1.)).bg(rgb(BORDER))),
        )
        .when_some(file_changes, |el, fc| {
            let changes_desc = format!(
                "{} file{} changed",
                fc.files,
                if fc.files == 1 { "" } else { "s" }
            );
            el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_size(px(11.))
                    .child(div().text_color(rgb(MUTED)).child(changes_desc))
                    .when(fc.additions > 0, |e| {
                        e.child(
                            div()
                                .text_color(rgb(SUCCESS))
                                .child(format!("+{}", fc.additions)),
                        )
                    })
                    .when(fc.deletions > 0, |e| {
                        e.child(
                            div()
                                .text_color(rgb(DANGER))
                                .child(format!("-{}", fc.deletions)),
                        )
                    })
                    .when(can_rewind && fc.state == "active", |e| {
                        e.child(undo_controls(row_id, &ent_id, confirming, cx))
                    }),
            )
        })
        .into_any_element()
}

/// Desktop-parity plan section (ConversationStatusPanel PlanStatusSection):
/// a collapsible section whose header row carries the title plus the
/// "{completed}/{total}" counter (green when complete); the item list scrolls
/// in its own 320px viewport so the panel never grows past its cap.
pub fn render_plan_checklist(
    plan: &PlanState,
    expanded: bool,
    cx: &mut Context<crate::app::root::RootView>,
) -> AnyElement {
    let total = plan.items.len();
    let completed = plan.completed_count();
    let is_completed = total > 0 && completed == total;
    let counter_color = if is_completed { SUCCESS } else { MUTED };

    div()
        .w_full()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(rgb(BORDER))
        .pt_2()
        .child(
            div()
                .id("plan-section")
                .h(px(32.))
                .min_w_0()
                .flex()
                .items_center()
                .gap_1p5()
                .px_2()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(crate::shared::theme::HOVER)))
                .rounded_sm()
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.plan_expanded = !this.plan_expanded;
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(TEXT))
                        .child("Progress"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(if expanded { "▾" } else { "▸" }),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(counter_color))
                        .child(format!("{completed}/{total}")),
                ),
        )
        .when(expanded, |el| {
            el.child(
                div()
                    .id("plan-items")
                    .max_h(px(320.))
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .pb_1p5()
                    .children(plan.items.iter().map(|item| {
                        let (icon, color) = match item.status.as_str() {
                            "completed" => ("✓", SUCCESS),
                            "inProgress" => ("●", ACCENT),
                            _ => ("○", MUTED),
                        };
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .text_size(px(11.5))
                            .child(div().text_color(rgb(color)).child(icon))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(if item.status == "completed" {
                                        rgb(MUTED)
                                    } else {
                                        rgb(TEXT)
                                    })
                                    .child(item.content.clone()),
                            )
                    })),
            )
        })
        .into_any_element()
}

fn small_btn(id: String, label: &str, color: u32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(id))
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(BORDER))
        .text_color(rgb(color))
        .cursor_pointer()
        .child(label.to_string())
}

/// Undo rewrites files on disk, so it takes a second click to confirm.
fn undo_controls(
    row_id: u64,
    entity_id: &str,
    confirming: bool,
    cx: &mut Context<crate::app::root::RootView>,
) -> AnyElement {
    let key = format!("undo:{row_id}");
    if !confirming {
        return small_btn(format!("undo-files-{row_id}"), "Undo", TEXT)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.confirm = Some(key.clone());
                cx.notify();
            }))
            .into_any_element();
    }
    let ent = entity_id.to_string();
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().text_color(rgb(DANGER)).child("Restore these files?"))
        .child(
            small_btn(format!("undo-confirm-{row_id}"), "Undo changes", DANGER).on_click(
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.confirm = None;
                    let ent = ent.clone();
                    this.state
                        .update(cx, |app, cx| app.apply_file_rewind(row_id, &ent, cx));
                }),
            ),
        )
        .child(
            small_btn(format!("undo-cancel-{row_id}"), "Cancel", MUTED).on_click(cx.listener(
                |this, _, _, cx| {
                    cx.stop_propagation();
                    this.confirm = None;
                    cx.notify();
                },
            )),
        )
        .into_any_element()
}
