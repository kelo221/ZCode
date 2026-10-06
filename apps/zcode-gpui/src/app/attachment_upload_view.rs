use crate::app::root::RootView;
use crate::backend::attachment_upload::UploadState;
use crate::shared::theme::{MUTED, ui_size};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn guard_input_ownership(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            state.retire_stale_uploads(cx);
            state.retire_stale_workflow_artifact_contents();
        });
        self.guard_child_focus(window, cx);
    }

    pub(crate) fn image_upload_feedback(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = self.state.read(cx);
        let mut uploads: Vec<_> = state
            .workspaces
            .iter()
            .flat_map(|ws| ws.image_uploads.values())
            .filter(|upload| state.upload_receipt_current(&upload.receipt, cx))
            .map(|upload| {
                (
                    upload.id.clone(),
                    upload.receipt.workspace.clone(),
                    matches!(upload.state, UploadState::Failed),
                    upload.format.extension(),
                )
            })
            .collect();
        uploads.sort_by(|a, b| a.0.cmp(&b.0));
        if uploads.is_empty() {
            return None;
        }
        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_1()
                .children(
                    uploads
                        .into_iter()
                        .map(|(id, workspace, failed, extension)| {
                            let cancel_id = id.clone();
                            let cancel_workspace = workspace.clone();
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(ui_size(12.)))
                                .text_color(rgb(MUTED))
                                .child(format!(
                                    "clipboard.{extension}: {}",
                                    if failed {
                                        crate::shared::i18n::label("Upload failed", "上传失败")
                                    } else {
                                        crate::shared::i18n::label("Uploading", "正在上传")
                                    }
                                ))
                                .children(failed.then(|| {
                                    let button = Button::new(
                                        format!("retry-upload-{id}"),
                                        crate::shared::i18n::label("Retry", "重试"),
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.state.update(cx, |state, cx| {
                                            state.retry_image_upload(&workspace, &id, cx)
                                        })
                                    }));
                                    let wrapper = div().child(button);
                                    #[cfg(test)]
                                    let wrapper = crate::app::test_support::track_children(
                                        wrapper,
                                        vec!["retry-image-upload".into()],
                                    );
                                    wrapper
                                }))
                                .child({
                                    let button = Button::new(
                                        format!("cancel-upload-{cancel_id}"),
                                        crate::shared::i18n::label("Cancel", "取消"),
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.state.update(cx, |state, cx| {
                                            state.cancel_image_upload(
                                                &cancel_workspace,
                                                &cancel_id,
                                                cx,
                                            )
                                        })
                                    }));
                                    let wrapper = div().child(button);
                                    #[cfg(test)]
                                    let wrapper = crate::app::test_support::track_children(
                                        wrapper,
                                        vec!["cancel-image-upload".into()],
                                    );
                                    wrapper
                                })
                        }),
                )
                .into_any_element(),
        )
    }
}
