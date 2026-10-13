//! Transcript row rendering for the chat surface: turns, user inputs,
//! assistant text with streaming cursor, collapsible reasoning, and expandable tool cards.

use crate::conversation::model::Row;
use crate::shared::theme::ui_size;
use crate::shared::theme::{BORDER, SURFACE, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

const USER_GROUP: &str = "user-message";

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
            } => Some(
                // Desktop ConversationUserInputBubble: right-aligned surface card
                // with a squared top-right corner; actions appear on hover.
                div()
                    .group(USER_GROUP)
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap_1()
                    .child(
                        div()
                            .max_w(px(576.))
                            .bg(rgb(SURFACE))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .rounded_tl(px(12.))
                            .rounded_bl(px(12.))
                            .rounded_br(px(12.))
                            .rounded_tr(px(2.))
                            .px_4()
                            .py_3()
                            .text_size(px(ui_size(14.)))
                            .text_color(rgb(TEXT))
                            .child(text.clone()),
                    )
                    .child(
                        self.user_message_actions(
                            *row_id,
                            entity_id.clone(),
                            text.clone(),
                            *can_edit && row_actions,
                            cx,
                        )
                        .opacity(0.)
                        .group_hover(USER_GROUP, |s| s.opacity(1.)),
                    )
                    .into_any_element(),
            ),
            Row::AssistantText {
                row_id,
                entity_id,
                text,
                state,
                can_retry,
            } => {
                let streaming = state == "streaming";
                let can_retry = *can_retry && !streaming && row_actions;
                Some(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap_1()
                        .text_size(px(ui_size(14.)))
                        .text_color(rgb(TEXT))
                        .child(crate::shared::markdown::render_markdown(
                            text, *row_id, streaming,
                        ))
                        .when(!streaming && !text.is_empty(), |el| {
                            el.child(self.assistant_message_actions(
                                *row_id,
                                entity_id.clone(),
                                text.clone(),
                                can_retry,
                                cx,
                            ))
                        })
                        .into_any_element(),
                )
            }
            Row::Reasoning {
                row_id,
                text,
                duration_ms,
            } => Some(self.render_reasoning(*row_id, text, *duration_ms, cx)),
            Row::ToolCall {
                row_id,
                label,
                status,
                input_text,
                output_text,
                tool_call_id,
            } => {
                let maybe_subagent = tool_call_id
                    .as_deref()
                    .and_then(|tcid| self.find_subagent_for_tool_call(tcid, cx));
                let tool_el =
                    self.render_tool_row(*row_id, label, status, input_text, output_text, cx);
                if let Some(subagent_row) = maybe_subagent {
                    let subagent_el = self.render_subagent_card(&subagent_row, true, cx);
                    Some(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .child(tool_el)
                            .child(subagent_el)
                            .into_any_element(),
                    )
                } else {
                    Some(tool_el)
                }
            }
            Row::Subagent {
                parent_tool_call_id,
                ..
            } => {
                let is_paired = parent_tool_call_id
                    .as_deref()
                    .is_some_and(|ptid| self.has_tool_call_with_id(ptid, cx));
                if is_paired {
                    None
                } else {
                    Some(self.render_subagent_card(row, false, cx))
                }
            }
            Row::Other => None,
        }
    }
}
