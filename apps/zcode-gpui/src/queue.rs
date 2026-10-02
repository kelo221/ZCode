//! Follow-up queue: snapshot parsing and the queue panel (commands live in
//! msg_actions.rs).
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (snapshot.ts:208-225, command.ts:166-174).

use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};
use serde_json::Value;

use crate::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, TEXT};

#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub queue_item_id: String,
    pub text: String,
    pub state: String,
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
    cx: &mut Context<crate::ui::RootView>,
) -> Option<AnyElement> {
    if queue.items.is_empty() {
        return None;
    }

    let count = queue.items.len();
    let auto_drain = queue.auto_drain;

    Some(
        div()
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
                                        .text_size(px(10.))
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
                            .text_size(px(10.))
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
            .child(div().flex().flex_col().gap_1().pt_2().children(
                queue.items.iter().enumerate().map(|(idx, item)| {
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
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child(item.text.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id(SharedString::from(format!("q-send-now-{idx}")))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(ACCENT))
                                        .text_size(px(10.))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(rgb(0x000000))
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
                                .child(
                                    div()
                                        .id(SharedString::from(format!("q-del-{idx}")))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(DANGER))
                                        .text_size(px(10.))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(rgb(0xffffff))
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
                }),
            ))
            .into_any_element(),
    )
}
