//! Transcript row rendering for the chat surface: turns, user inputs,
//! assistant text with streaming cursor, collapsible reasoning, and expandable tool cards.

use crate::conversation::model::Row;
use crate::shared::theme::{ACCENT, BORDER, CARD, MUTED, REASONING, TEXT, USER_BLUE};
use gpui::{
    AnyElement, ClipboardItem, Context, ElementId, IntoElement, ParentElement, Styled, div,
    prelude::*, px, rgb,
};

impl crate::app::root::RootView {
    /// `row_actions` is false until the snapshot's log epoch is known; row
    /// commands cannot be sent without `baseLogEpoch`.
    pub(crate) fn transcript_row(
        &self,
        row: &Row,
        row_actions: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match row {
            Row::TurnHeader {
                row_id,
                entity_id,
                state,
                active_ms,
                file_changes,
                can_rewind,
            } => Some(crate::conversation::turn_meta::render_turn_header(
                (*row_id, entity_id),
                state,
                *active_ms,
                file_changes.as_ref(),
                *can_rewind && row_actions,
                self.confirm.as_deref() == Some(format!("undo:{row_id}").as_str()),
                cx,
            )),
            Row::UserInput {
                row_id,
                entity_id,
                text,
                can_edit,
            } => {
                let edit_entity = entity_id.clone();
                let user_text = text.clone();
                let copy_text = text.clone();
                let edit_text = text.clone();
                let r_id = *row_id;
                let c_edit = *can_edit && row_actions;

                Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap_1()
                        .child(
                            div()
                                .max_w(px(640.))
                                .bg(rgb(CARD))
                                .rounded_lg()
                                .px_3()
                                .py_2()
                                .text_size(px(13.5))
                                .text_color(rgb(USER_BLUE))
                                .child(user_text),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .opacity(0.8)
                                .child(
                                    div()
                                        .id(ElementId::NamedInteger("copy-user".into(), r_id))
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_size(px(10.))
                                        .text_color(rgb(MUTED))
                                        .hover(|s| s.text_color(rgb(TEXT)))
                                        .cursor_pointer()
                                        .child("Copy")
                                        .on_click(cx.listener(move |_this, _, _, cx| {
                                            cx.stop_propagation();
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy_text.clone(),
                                            ));
                                        })),
                                )
                                .when(c_edit, |el| {
                                    el.child(
                                        div()
                                            .id(ElementId::NamedInteger("edit-user".into(), r_id))
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_size(px(10.))
                                            .text_color(rgb(MUTED))
                                            .hover(|s| s.text_color(rgb(TEXT)))
                                            .cursor_pointer()
                                            .child("Edit")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                let (t, e) =
                                                    (edit_text.clone(), edit_entity.clone());
                                                this.state.update(cx, |s, cx| {
                                                    s.begin_edit(r_id, &e, &t, cx);
                                                });
                                            })),
                                    )
                                }),
                        )
                        .into_any_element(),
                )
            }
            Row::AssistantText {
                row_id,
                entity_id,
                text,
                state,
                can_retry,
            } => {
                let streaming = state == "streaming";
                let copy_text = text.clone();
                let r_id = *row_id;
                let ent_id = entity_id.clone();
                let c_retry = *can_retry && !streaming && row_actions;

                Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_1()
                        .child(crate::shared::markdown::render_markdown(
                            text, *row_id, streaming,
                        ))
                        .when(!streaming && !text.is_empty(), |el| {
                            el.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .opacity(0.8)
                                    .child(
                                        div()
                                            .id(ElementId::NamedInteger("copy-asst".into(), r_id))
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_size(px(10.))
                                            .text_color(rgb(MUTED))
                                            .hover(|s| s.text_color(rgb(TEXT)))
                                            .cursor_pointer()
                                            .child("Copy")
                                            .on_click(cx.listener(move |_this, _, _, cx| {
                                                cx.stop_propagation();
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy_text.clone(),
                                                ));
                                            })),
                                    )
                                    .when(c_retry, |actions| {
                                        let target_ent = ent_id.clone();
                                        let ent_fork = ent_id.clone();
                                        let ent_like = ent_id.clone();
                                        let ent_dislike = ent_id.clone();
                                        actions
                                            .child(
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "retry-turn".into(),
                                                        r_id,
                                                    ))
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(MUTED))
                                                    .hover(|s| s.text_color(rgb(ACCENT)))
                                                    .cursor_pointer()
                                                    .child("Retry")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            let e_id = target_ent.clone();
                                                            this.state.update(cx, |app, cx| {
                                                                app.retry_turn(r_id, &e_id, cx);
                                                            });
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "fork-turn".into(),
                                                        r_id,
                                                    ))
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(MUTED))
                                                    .hover(|s| s.text_color(rgb(ACCENT)))
                                                    .cursor_pointer()
                                                    .child("Fork")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            let e_id = ent_fork.clone();
                                                            this.state.update(cx, |app, cx| {
                                                                app.fork_assistant(r_id, &e_id, cx);
                                                            });
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "like-turn".into(),
                                                        r_id,
                                                    ))
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(MUTED))
                                                    .hover(|s| s.text_color(rgb(ACCENT)))
                                                    .cursor_pointer()
                                                    .child("👍")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            let e_id = ent_like.clone();
                                                            this.state.update(cx, |app, cx| {
                                                                app.set_assistant_feedback(
                                                                    r_id,
                                                                    &e_id,
                                                                    Some("like"),
                                                                    cx,
                                                                );
                                                            });
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::NamedInteger(
                                                        "dislike-turn".into(),
                                                        r_id,
                                                    ))
                                                    .px_1p5()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(MUTED))
                                                    .hover(|s| s.text_color(rgb(ACCENT)))
                                                    .cursor_pointer()
                                                    .child("👎")
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            let e_id = ent_dislike.clone();
                                                            this.state.update(cx, |app, cx| {
                                                                app.set_assistant_feedback(
                                                                    r_id,
                                                                    &e_id,
                                                                    Some("dislike"),
                                                                    cx,
                                                                );
                                                            });
                                                        },
                                                    )),
                                            )
                                    }),
                            )
                        })
                        .into_any_element(),
                )
            }
            Row::Reasoning {
                row_id,
                text,
                duration_ms,
            } => {
                let r_id = *row_id;
                let expanded = self.expanded_reasonings.contains(&r_id);
                let duration_desc = duration_ms
                    .map(|ms| format!(" ({})", crate::conversation::turn_meta::format_duration(ms)))
                    .unwrap_or_default();

                let header_title = format!("💭 Thought{duration_desc}");

                Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .my_1()
                        .child(
                            div()
                                .id(ElementId::NamedInteger("toggle-thought".into(), r_id))
                                .flex()
                                .items_center()
                                .gap_1()
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .text_color(rgb(REASONING))
                                .child(if expanded { "▼" } else { "▶" })
                                .child(header_title)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if this.expanded_reasonings.contains(&r_id) {
                                        this.expanded_reasonings.remove(&r_id);
                                    } else {
                                        this.expanded_reasonings.insert(r_id);
                                    }
                                    cx.notify();
                                })),
                        )
                        .when(expanded, |el| {
                            el.child(
                                div()
                                    .pl_3()
                                    .border_l_1()
                                    .border_color(rgb(BORDER))
                                    .text_size(px(12.))
                                    .text_color(rgb(REASONING))
                                    .opacity(0.85)
                                    .child(text.clone()),
                            )
                        })
                        .into_any_element(),
                )
            }
            Row::ToolCall {
                row_id,
                label,
                status,
                input_text,
                output_text,
            } => Some(self.render_tool_row(*row_id, label, status, input_text, output_text, cx)),
            Row::Other => None,
        }
    }
}
