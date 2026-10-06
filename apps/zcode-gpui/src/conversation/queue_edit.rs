use crate::app::store::AppState;
use crate::backend::{submission::Submission, workspace::Pending};
use crate::conversation::msg_actions::ComposerIntent;
use gpui::Context;
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct QueueRestore {
    pub submission: Submission,
    pub item_id: String,
}

impl AppState {
    pub(crate) fn queue_edit_pending(&self, workspace: &str, sid: &str) -> bool {
        self.ws(workspace).is_some_and(|ws| {
            ws.pending.values().any(|pending| {
                matches!(pending,
            Pending::QueueEdit(restore) if restore.submission.draft_key == sid)
            })
        })
    }

    pub(crate) fn restore_queued_input(
        &mut self,
        workspace: &str,
        sid: &str,
        item_id: &str,
        cx: &mut Context<Self>,
    ) {
        let composer = self.composer.read(cx);
        if self.active_ws_key().as_deref() != Some(workspace)
            || self.active.as_deref() != Some(sid)
            || self.is_read_only_view()
            || self.composer_intent != ComposerIntent::Send
            || !composer.text().is_empty()
            || !composer.attachments().is_empty()
            || self.queue_edit_pending(workspace, sid)
        {
            self.push_error(
                crate::shared::i18n::label(
                    "Clear the composer before editing queued input",
                    "编辑排队输入前请清空输入框",
                )
                .into(),
            );
            return;
        }
        let Some(conv) = self
            .conversations
            .get(sid)
            .filter(|conv| conv.revision_known && conv.queue_edit_allowed)
        else {
            return;
        };
        let Some(item) = conv
            .queue
            .as_ref()
            .and_then(|queue| {
                queue
                    .items
                    .iter()
                    .find(|item| item.queue_item_id == item_id)
            })
            .filter(|item| {
                item.state == "queued"
                    && matches!(item.kind.as_str(), "sendText" | "sendGoalCommand")
            })
        else {
            return;
        };
        let text =
            if item.kind == "sendGoalCommand" && !item.text.trim_start().starts_with("/goal ") {
                format!("/goal {}", item.text)
            } else {
                item.text.clone()
            };
        let restore = QueueRestore {
            item_id: item_id.into(),
            submission: Submission {
                workspace: workspace.into(),
                draft_key: sid.into(),
                navigation_generation: self.navigation_generation,
                text,
                attachments: item.attachments.clone(),
                config: item.config.clone(),
            },
        };
        let params = crate::backend::session_cmds::command_params(
            &self.client_id,
            Some(sid),
            "deleteQueueItem",
            json!({"queueItemId":item_id}),
            Some(conv.revision),
            None,
            crate::backend::launcher::new_command_id(),
            crate::backend::launcher::now_ms(),
        );
        self.send_envelope(workspace, params, Pending::QueueEdit(restore));
        cx.notify();
    }

    pub(crate) fn settle_queue_edit(
        &mut self,
        restore: QueueRestore,
        ack: Option<&Value>,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            ack.and_then(|ack| ack.get("status"))
                .and_then(Value::as_str),
            Some("accepted" | "duplicate")
        ) {
            let submission = restore.submission;
            let binding_matches = self.active_ws_key().as_deref()
                == Some(submission.workspace.as_str())
                && self.active.as_deref() == Some(submission.draft_key.as_str())
                && self.navigation_generation == submission.navigation_generation
                && self.composer_intent == ComposerIntent::Send
                && !self.is_read_only_view();
            let empty = self.composer.read(cx).text().is_empty()
                && self.composer.read(cx).attachments().is_empty();
            if binding_matches && empty {
                if !submission.config.is_null() {
                    self.draft_submission_overrides
                        .insert(submission.draft_key.clone(), submission.config);
                }
                self.composer.update(cx, |composer, cx| {
                    composer.set_text(&submission.text);
                    composer.attachments = submission.attachments;
                    cx.notify();
                });
            } else {
                // ACK 到达时用户可能已写新草稿；撤回数据只保存到原绑定，不能覆盖可见输入。
                self.save_recovered_submission(submission);
                self.push_error(
                    crate::shared::i18n::label(
                        "Queued input restored to its original session draft",
                        "排队输入已恢复到原会话草稿",
                    )
                    .into(),
                );
            }
        } else {
            self.push_error(format!(
                "{}: {}",
                crate::shared::i18n::label(
                    "Queued input could not be restored",
                    "无法撤回排队输入"
                ),
                restore.item_id
            ));
        }
        cx.notify();
    }
}

pub(crate) fn edit_button(
    item: &crate::conversation::queue::QueueItem,
    state: &gpui::Entity<AppState>,
    cx: &mut Context<crate::app::root::RootView>,
) -> Option<gpui::AnyElement> {
    use gpui::{IntoElement, ParentElement};
    if item.state != "queued" || !matches!(item.kind.as_str(), "sendText" | "sendGoalCommand") {
        return None;
    }
    // render 中 RootView 已被借用；通过传入的 AppState 读取，避免递归读取 RootView 导致 panic。
    let state = state.read(cx);
    let (workspace, sid) = state.active_ws_key().zip(state.active.clone())?;
    let allowed = state
        .conversations
        .get(&sid)
        .is_some_and(|conv| conv.queue_edit_allowed && conv.revision_known);
    let pending = state.queue_edit_pending(&workspace, &sid);
    let id = item.queue_item_id.clone();
    let element = gpui::div().child(
        ely_gpui_component::buttons::Button::new(
            gpui::SharedString::from(format!("q-edit-{id}")),
            crate::shared::i18n::label("Edit", "编辑"),
        )
        .size(ely_gpui_component::theme::ControlSize::Sm)
        .disabled(!allowed || pending)
        .loading(pending)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.state.update(cx, |state, cx| {
                state.restore_queued_input(&workspace, &sid, &id, cx)
            });
            let focus = this.state.read(cx).composer.read(cx).focus.clone();
            window.focus(&focus, cx);
        })),
    );
    #[cfg(test)]
    let element = crate::app::test_support::track_children(
        element,
        vec![format!("q-edit-{}", item.queue_item_id)],
    );
    Some(element.into_any_element())
}

#[cfg(test)]
#[path = "queue_edit_tests.rs"]
mod tests;
