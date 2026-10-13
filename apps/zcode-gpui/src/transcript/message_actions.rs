//! Icon actions under user and assistant messages (copy, edit, retry, fork,
//! feedback), drawn as Ely ghost icon buttons like Desktop's message toolbar.

use crate::app::root::RootView;
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{ClickEvent, ClipboardItem, Context, Div, ElementId, Window, div, prelude::*};

fn action(
    id: &'static str,
    row: u64,
    glyph: IconName,
    tip: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> IconButton {
    IconButton::new(ElementId::NamedInteger(id.into(), row), glyph)
        .size(ControlSize::Sm)
        .variant(ButtonVariant::Ghost)
        .tooltip(tip)
        .on_click(on_click)
}

fn copy_action(id: &'static str, row: u64, text: String, cx: &mut Context<RootView>) -> IconButton {
    action(
        id,
        row,
        IconName::Copy,
        "Copy",
        cx.listener(move |_, _, _, cx| {
            cx.stop_propagation();
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        }),
    )
}

impl RootView {
    pub(crate) fn user_message_actions(
        &self,
        row: u64,
        entity: String,
        text: String,
        can_edit: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let edit_text = text.clone();
        div()
            .flex()
            .items_center()
            .gap_0p5()
            .child(copy_action("copy-user", row, text, cx))
            .when(can_edit, |el| {
                el.child(action(
                    "edit-user",
                    row,
                    IconName::Pencil,
                    "Edit",
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let (t, e) = (edit_text.clone(), entity.clone());
                        this.state.update(cx, |s, cx| s.begin_edit(row, &e, &t, cx));
                    }),
                ))
            })
    }

    pub(crate) fn assistant_message_actions(
        &self,
        row: u64,
        entity: String,
        text: String,
        can_retry: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (retry, fork, like, dislike) = (
            entity.clone(),
            entity.clone(),
            entity.clone(),
            entity.clone(),
        );
        div()
            .flex()
            .items_center()
            .gap_0p5()
            .child(copy_action("copy-asst", row, text, cx))
            .when(can_retry, |el| {
                el.child(action(
                    "retry-turn",
                    row,
                    IconName::RotateCcw,
                    "Retry",
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.state
                            .update(cx, |app, cx| app.retry_turn(row, &retry, cx));
                    }),
                ))
                .child(action(
                    "fork-turn",
                    row,
                    IconName::GitBranch,
                    "Fork",
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.state
                            .update(cx, |app, cx| app.fork_assistant(row, &fork, cx));
                    }),
                ))
                .child(action(
                    "like-turn",
                    row,
                    IconName::ThumbsUp,
                    "Good response",
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.state.update(cx, |app, cx| {
                            app.set_assistant_feedback(row, &like, Some("like"), cx)
                        });
                    }),
                ))
                .child(action(
                    "dislike-turn",
                    row,
                    IconName::ThumbsDown,
                    "Bad response",
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.state.update(cx, |app, cx| {
                            app.set_assistant_feedback(row, &dislike, Some("dislike"), cx)
                        });
                    }),
                ))
            })
    }
}
