//! Composer attachment tray and attachment picker buttons.

use crate::app::root::RootView;
use crate::composer::attachment::{
    ATTACHMENT_MAX_BYTES, AttachmentRef, detect_mime_type, format_bytes,
};
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD_HOVER, DANGER, I_ADD, MUTED, TEXT, icon};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Div, IntoElement, ParentElement,
    PathPromptOptions, SharedString, Styled, div, prelude::*, px,
};
use std::path::PathBuf;

impl RootView {
    /// Render the horizontal tray of attached files inside or above the composer.
    pub(crate) fn render_attachment_tray(
        composer: &gpui::Entity<crate::composer::input::Composer>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let attachments = composer.read(cx).attachments().to_vec();
        if attachments.is_empty() {
            return None;
        }

        let comp_entity = composer.clone();
        Some(
            div()
                .id("attachment-tray")
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1p5()
                .w_full()
                .pb_1()
                .children(attachments.into_iter().enumerate().map(|(idx, att)| {
                    let name = att.file_name;
                    let size_str = format_bytes(att.bytes);
                    let comp = comp_entity.clone();

                    div()
                        .id(SharedString::from(format!("att-chip-{idx}")))
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(rgb(BORDER))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .hover(|s| s.bg(rgb(CARD_HOVER)))
                        .child(
                            div()
                                .text_size(px(ui_size(11.)))
                                .text_color(rgb(ACCENT))
                                .child("📎"),
                        )
                        .child(
                            div()
                                .max_w(px(160.))
                                .truncate()
                                .text_size(px(ui_size(11.5)))
                                .text_color(rgb(TEXT))
                                .child(name),
                        )
                        .child(
                            div()
                                .text_size(px(ui_size(10.)))
                                .text_color(rgb(MUTED))
                                .child(format!("({size_str})")),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("att-del-{idx}")))
                                .cursor(CursorStyle::PointingHand)
                                .text_size(px(ui_size(11.)))
                                .text_color(rgb(MUTED))
                                .hover(|s| s.text_color(rgb(DANGER)))
                                .child("✕")
                                .on_click(cx.listener(
                                    move |_this, _: &ClickEvent, _window, cx| {
                                        cx.stop_propagation();
                                        comp.update(cx, |c, cx| {
                                            c.remove_attachment(idx, cx);
                                        });
                                    },
                                )),
                        )
                }))
                .into_any_element(),
        )
    }

    /// Attach button with "+" icon triggering native file prompt (desktop parity).
    pub(crate) fn render_attach_button(cx: &mut Context<Self>) -> gpui::Stateful<Div> {
        div()
            .id("attach-btn")
            .size(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor(CursorStyle::PointingHand)
            .hover(|s| s.bg(rgb(CARD_HOVER)))
            .child(icon(I_ADD, 13., MUTED))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.prompt_attach_files(cx);
            }))
    }

    /// Prompt user to select files from disk and attach them to the composer.
    pub(crate) fn prompt_attach_files(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await {
                this.update(cx, |this, cx| {
                    this.attach_files(paths, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Validate and attach local file paths to the composer.
    pub(crate) fn attach_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let composer = self.state.read(cx).composer.clone();

        for path in paths {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "attachment".into());

            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    self.state.update(cx, |s, _cx| {
                        s.push_error(format!("Could not read {}: {}", file_name, e));
                    });
                    continue;
                }
            };

            let bytes = metadata.len();
            if bytes > ATTACHMENT_MAX_BYTES as u64 {
                self.state.update(cx, |s, _cx| {
                    s.push_error(format!(
                        "Attachment '{}' ({}) exceeds 20 MB limit",
                        file_name,
                        format_bytes(bytes)
                    ));
                });
                continue;
            }

            let mime = detect_mime_type(&file_name).to_string();
            let att = AttachmentRef {
                reference: path.to_string_lossy().to_string(),
                file_name,
                mime,
                bytes,
                preview_ref: None,
            };

            composer.update(cx, |c, cx| {
                c.add_attachment(att, cx);
            });
        }
        cx.notify();
    }
}
