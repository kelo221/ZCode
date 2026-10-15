//! User-input rows: the right-aligned message bubble (Desktop
//! `ConversationUserInputBubble`) with its send time, and background results
//! (`<task-notification>` text) as a compact left-aligned notice instead of
//! raw XML.

use crate::shared::local_time::format_timestamp;
use crate::shared::theme::{BORDER, MUTED, SUBTLEST, SURFACE, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use crate::transcript::tagged_text::{parse_task_notification, strip_system_reminders};
use gpui::{AnyElement, Context, Div, FontWeight, div, prelude::*, px};

const USER_GROUP: &str = "user-message";

pub(crate) struct UserRowView<'a> {
    pub row_id: u64,
    pub entity_id: &'a str,
    pub text: &'a str,
    pub can_edit: bool,
    pub origin: &'a str,
    pub created_at: Option<u64>,
    pub attachments: &'a [crate::composer::attachment::AttachmentRef],
}

/// Attachment chips under the bubble. Only the reference reaches the
/// transcript; the bytes stay with the backend, so no inline preview.
fn attachment_chips(attachments: &[crate::composer::attachment::AttachmentRef]) -> Option<Div> {
    (!attachments.is_empty()).then(|| {
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap_1()
            .max_w(px(576.))
            .children(attachments.iter().map(|a| {
                let kind = if a.mime.starts_with("image/") {
                    "Image"
                } else {
                    "File"
                };
                div()
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "{kind}: {} · {}",
                        a.file_name,
                        crate::composer::attachment::format_bytes(a.bytes)
                    ))
            }))
    })
}

pub(crate) fn timestamp_label(created_at: Option<u64>) -> Option<Div> {
    created_at.map(|ms| {
        div()
            .flex_none()
            .text_size(px(ui_size(11.)))
            .text_color(rgb(SUBTLEST))
            .child(format_timestamp(ms))
    })
}

fn notification_card(text: &str, created_at: Option<u64>, row_id: u64) -> Option<AnyElement> {
    let n = parse_task_notification(text)?;
    let title = match (&n.description, n.task_id.is_empty()) {
        (Some(d), _) => d.clone(),
        (None, false) => n.task_id.clone(),
        (None, true) => "Background task".to_string(),
    };
    let body = n.body();
    Some(
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("Background result · {}", n.status)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(title))
                    .children(timestamp_label(created_at)),
            )
            .when(!body.is_empty(), |el| {
                el.child(crate::shared::markdown::render_passive_markdown(
                    &body, row_id,
                ))
            })
            .into_any_element(),
    )
}

impl crate::app::root::RootView {
    pub(crate) fn render_user_row(
        &self,
        v: UserRowView<'_>,
        row_actions: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let looks_like_notice = v.origin == "backgroundResult"
            || v.text.trim_start().starts_with("<task-notification>");
        if looks_like_notice && let Some(card) = notification_card(v.text, v.created_at, v.row_id) {
            return Some(card);
        }
        let shown = strip_system_reminders(v.text);
        if shown.is_empty() && !v.text.is_empty() {
            return None;
        }
        Some(
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
                        .min_w_0()
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
                        .child(crate::shared::selectable::SelectableText::new(
                            gpui::ElementId::NamedInteger("user-text".into(), v.row_id),
                            shown.clone(),
                        )),
                )
                .children(attachment_chips(v.attachments))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            self.user_message_actions(
                                v.row_id,
                                v.entity_id.to_string(),
                                shown,
                                v.can_edit && row_actions,
                                cx,
                            )
                            .opacity(0.)
                            .group_hover(USER_GROUP, |s| s.opacity(1.)),
                        )
                        .children(timestamp_label(v.created_at)),
                )
                .into_any_element(),
        )
    }
}
