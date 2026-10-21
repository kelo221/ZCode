//! Follow-up queue: snapshot parsing and the queue panel (commands live in
//! conversation/msg_actions.rs).
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (snapshot.ts:208-225, command.ts:166-174).

use crate::shared::theme::ui_size;
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use serde_json::Value;

use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, TEXT};

#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub queue_item_id: String,
    pub text: String,
    pub state: String,
    pub kind: String,
    pub attachments: Vec<crate::composer::attachment::AttachmentRef>,
    pub config: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueueState {
    pub items: Vec<QueueItem>,
    pub auto_drain: bool,
    pub pause_reason: Option<String>,
}

impl QueueState {
    pub fn from_value(v: &Value) -> Option<Self> {
        let items = v
            .get("items")
            .and_then(Value::as_array)?
            .iter()
            .filter_map(|it| {
                let queue_item_id = it.get("queueItemId").and_then(Value::as_str)?.to_string();
                let text = it
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let state = it
                    .get("dispatch")
                    .and_then(|d| d.get("state"))
                    .and_then(Value::as_str)
                    .unwrap_or("queued")
                    .to_string();
                Some(QueueItem {
                    queue_item_id,
                    text,
                    state,
                    kind: it
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("sendText")
                        .into(),
                    attachments: serde_json::from_value(
                        it.get("attachments")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!([])),
                    )
                    .ok()?,
                    config: Value::Object(
                        ["modelSelection", "mode", "planEnabled"]
                            .into_iter()
                            .filter_map(|key| {
                                it.get(key).cloned().map(|value| (key.to_owned(), value))
                            })
                            .collect(),
                    ),
                })
            })
            .collect();
        let auto_drain = v.get("autoDrain").and_then(Value::as_bool).unwrap_or(true);
        let pause_reason = v
            .get("pauseReason")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(Self {
            items,
            auto_drain,
            pause_reason,
        })
    }
}

/// Render the follow-up queue panel if there are queued items.
pub fn render_queue_panel(
    queue: &QueueState,
    state: &gpui::Entity<crate::app::store::AppState>,
    cx: &mut Context<crate::app::root::RootView>,
) -> Option<AnyElement> {
    if queue.items.is_empty() {
        return None;
    }

    let count = queue.items.len();
    let auto_drain = queue.auto_drain;

    let panel = div()
        .w_full()
        .flex()
        .flex_col()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_md()
        .p_2()
        .my_2()
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
                                .text_xs()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(rgb(TEXT))
                                .child(format!("Queued ({count})")),
                        )
                        .when(!auto_drain, |el| {
                            el.child(
                                div()
                                    .text_size(px(ui_size(10.)))
                                    .px_1()
                                    .rounded_sm()
                                    .bg(rgb(BORDER))
                                    .text_color(rgb(MUTED))
                                    .child("Paused"),
                            )
                        }),
                )
                .child(
                    div()
                        .id("toggle-drain-btn")
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .bg(rgb(BORDER))
                        .text_size(px(ui_size(10.)))
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .child(if auto_drain { "Pause" } else { "Resume" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let new_val = !auto_drain;
                            this.state.update(cx, |app, cx| {
                                app.set_auto_drain(new_val, cx);
                            });
                        })),
                ),
        )
        .child(
            div()
                .id("queue-items")
                .flex()
                .flex_col()
                .gap_1()
                .mt_2()
                .max_h(px(QUEUE_LIST_MAX_HEIGHT))
                .min_h_0()
                .overflow_y_scroll()
                .children(queue.items.iter().enumerate().map(|(idx, item)| {
                    let item_id = item.queue_item_id.clone();
                    let item_id_del = item.queue_item_id.clone();

                    let item_id_send = item_id.clone();
                    div()
                        .id(SharedString::from(format!("queue-item-{idx}")))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .bg(rgb(BORDER))
                        .flex_none()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child(queue_preview(&item.text)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .children(crate::conversation::queue_edit::edit_button(
                                    item, state, cx,
                                ))
                                .child(
                                    div()
                                        .id(SharedString::from(format!("q-send-now-{idx}")))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(ACCENT))
                                        .text_size(px(ui_size(10.)))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(crate::shared::theme::active_theme().inverse)
                                        .cursor_pointer()
                                        .child("Send now")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            let i_id = item_id_send.clone();
                                            this.state.update(cx, |app, cx| {
                                                app.send_queued_now(&i_id, cx);
                                            });
                                        })),
                                )
                                .when(idx > 0, |btns| {
                                    let i_id = item_id.clone();
                                    let before_id = queue.items[idx - 1].queue_item_id.clone();
                                    btns.child(
                                        div()
                                            .id(SharedString::from(format!("q-up-{idx}")))
                                            .px_1()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(MUTED))
                                            .text_size(px(ui_size(10.)))
                                            .text_color(
                                                crate::shared::theme::active_theme().inverse,
                                            )
                                            .cursor_pointer()
                                            .child("↑")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                let id = i_id.clone();
                                                let before = before_id.clone();
                                                this.state.update(cx, |app, cx| {
                                                    app.reorder_queue_item(&id, Some(&before), cx);
                                                });
                                            })),
                                    )
                                })
                                .when(idx + 1 < count, |btns| {
                                    let i_id = item_id.clone();
                                    let before_id = if idx + 2 < count {
                                        Some(queue.items[idx + 2].queue_item_id.clone())
                                    } else {
                                        None
                                    };
                                    btns.child(
                                        div()
                                            .id(SharedString::from(format!("q-down-{idx}")))
                                            .px_1()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(rgb(MUTED))
                                            .text_size(px(ui_size(10.)))
                                            .text_color(
                                                crate::shared::theme::active_theme().inverse,
                                            )
                                            .cursor_pointer()
                                            .child("↓")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                let id = i_id.clone();
                                                let before = before_id.clone();
                                                this.state.update(cx, |app, cx| {
                                                    app.reorder_queue_item(
                                                        &id,
                                                        before.as_deref(),
                                                        cx,
                                                    );
                                                });
                                            })),
                                    )
                                })
                                .child(
                                    div()
                                        .id(SharedString::from(format!("q-del-{idx}")))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(DANGER))
                                        .text_size(px(ui_size(10.)))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(gpui::white())
                                        .cursor_pointer()
                                        .child("✕")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            let i_id = item_id_del.clone();
                                            this.state.update(cx, |app, cx| {
                                                app.delete_queue_item(&i_id, cx);
                                            });
                                        })),
                                ),
                        )
                })),
        );
    #[cfg(test)]
    let panel = crate::app::test_support::track_children(
        panel,
        vec!["queue-header".into(), "queue-items".into()],
    );
    Some(panel.into_any_element())
}

/// Row list cap: one-line rows keep this to about six items before scrolling.
const QUEUE_LIST_MAX_HEIGHT: f32 = 200.;
const QUEUE_PREVIEW_MAX_CHARS: usize = 200;

/// One-line queue row text (Desktop `ConversationQueuePanel` `truncate`):
/// whitespace runs collapse to one space and long text is capped, so a huge
/// paste neither wraps the panel to window height nor shapes 15k chars a frame.
pub(crate) fn queue_preview(text: &str) -> SharedString {
    let mut out = String::new();
    let mut chars = 0;
    for word in text.split_whitespace() {
        if chars >= QUEUE_PREVIEW_MAX_CHARS {
            out.push('…');
            return out.into();
        }
        if !out.is_empty() {
            out.push(' ');
            chars += 1;
        }
        for ch in word.chars() {
            if chars >= QUEUE_PREVIEW_MAX_CHARS {
                out.push('…');
                return out.into();
            }
            out.push(ch);
            chars += 1;
        }
    }
    out.into()
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
