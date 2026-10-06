use crate::app::store::AppState;
use crate::composer::attachment::{self, AttachmentRef};
use gpui::{App, Context};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const UPLOAD_ERROR: &str = "Image upload failed; Retry or Cancel to keep editing";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UploadStep {
    Begin,
    Chunk(usize),
    Commit,
}

pub(crate) enum UploadState {
    Preparing,
    Sending(UploadStep),
    Failed,
}

#[derive(Clone)]
pub(crate) struct UploadReceipt {
    pub workspace: String,
    pub session: String,
    pub connection: String,
    pub process: u64,
    pub navigation: u64,
    pub replacement: u64,
}

pub(crate) struct PreparedImage {
    pub chunks: Vec<String>,
    pub checksum: String,
}

pub(crate) struct ImageUpload {
    pub id: String,
    pub receipt: UploadReceipt,
    pub bytes: Arc<Vec<u8>>,
    pub format: gpui::ImageFormat,
    pub prepared: Option<PreparedImage>,
    pub state: UploadState,
    pub began: bool,
}

pub(crate) type ImageUploads = HashMap<String, ImageUpload>;

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
enum BeginResult {
    #[serde(rename_all = "camelCase")]
    Staging {
        upload_id: String,
        next_chunk_index: usize,
    },
    #[serde(rename_all = "camelCase")]
    Committed {
        upload_id: String,
        next_chunk_index: usize,
        #[serde(rename = "ref")]
        reference: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChunkResult {
    upload_id: String,
    next_chunk_index: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommitResult {
    #[serde(rename = "ref")]
    reference: String,
}

impl ImageUpload {
    pub(crate) fn accept_response(
        &self,
        step: UploadStep,
        value: Value,
    ) -> Result<Option<String>, ()> {
        let count = self.prepared.as_ref().ok_or(())?.chunks.len();
        match step {
            UploadStep::Begin => {
                match serde_json::from_value::<BeginResult>(value).map_err(|_| ())? {
                    BeginResult::Staging {
                        upload_id,
                        next_chunk_index,
                    } if upload_id == self.id && next_chunk_index == 0 => Ok(None),
                    BeginResult::Committed {
                        upload_id,
                        next_chunk_index,
                        reference,
                    } if upload_id == self.id
                        && next_chunk_index == count
                        && valid_artifact_ref(&reference) =>
                    {
                        Ok(Some(reference))
                    }
                    _ => Err(()),
                }
            }
            UploadStep::Chunk(index) => {
                let result: ChunkResult = serde_json::from_value(value).map_err(|_| ())?;
                if result.upload_id == self.id
                    && result.next_chunk_index == index + 1
                    && index < count
                {
                    Ok(None)
                } else {
                    Err(())
                }
            }
            UploadStep::Commit => {
                let result: CommitResult = serde_json::from_value(value).map_err(|_| ())?;
                valid_artifact_ref(&result.reference)
                    .then_some(Some(result.reference))
                    .ok_or(())
            }
        }
    }

    pub(crate) fn attachment(&self, reference: String) -> AttachmentRef {
        AttachmentRef {
            reference,
            file_name: format!("clipboard.{}", self.format.extension()),
            mime: self.format.mime_type().into(),
            bytes: self.bytes.len() as u64,
            preview_ref: None,
        }
    }
}

fn valid_artifact_ref(reference: &str) -> bool {
    !reference.trim().is_empty()
        && reference == reference.trim()
        && !reference.chars().any(char::is_control)
        && url::Url::parse(reference).is_ok_and(|url| {
            url.scheme() == "zcode-artifact"
                && url.host_str().is_some()
                && url.path().len() > 1
                && url.query().is_none()
                && url.fragment().is_none()
        })
}

impl AppState {
    pub(crate) fn upload_receipt_current(&self, receipt: &UploadReceipt, cx: &App) -> bool {
        !self.is_read_only_view()
            && self.composer_intent == crate::conversation::msg_actions::ComposerIntent::Send
            && self.active.as_deref() == Some(receipt.session.as_str())
            && self.active_ws_key().as_deref() == Some(receipt.workspace.as_str())
            && self.navigation_generation == receipt.navigation
            && self.composer.read(cx).replacement_generation == receipt.replacement
            && self.ws(&receipt.workspace).is_some_and(|ws| {
                ws.generation == receipt.process
                    && ws.connection_id == receipt.connection
                    && ws.started
                    && ws.inbound.is_some()
            })
    }

    pub(crate) fn uploads_block_submission(&self, cx: &App) -> bool {
        self.workspaces.iter().any(|ws| {
            ws.image_uploads
                .values()
                .any(|upload| self.upload_receipt_current(&upload.receipt, cx))
        })
    }

    pub(crate) fn block_upload_submission(&mut self, cx: &mut Context<Self>) -> bool {
        self.retire_stale_uploads(cx);
        if !self.uploads_block_submission(cx) {
            return false;
        }
        self.push_error(
            crate::shared::i18n::label(
                "Finish or cancel image uploads before sending; draft kept",
                "发送前请完成或取消图片上传，草稿已保留",
            )
            .into(),
        );
        cx.notify();
        true
    }

    pub(crate) fn start_image_upload(
        &mut self,
        bytes: Vec<u8>,
        format: gpui::ImageFormat,
        replacement: u64,
        cx: &mut Context<Self>,
    ) {
        if self.composer.read(cx).replacement_generation != replacement || self.is_read_only_view()
        {
            return;
        }
        if self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send {
            self.push_error("Image paste is unavailable while editing or renaming".into());
            cx.notify();
            return;
        }
        if bytes.is_empty() || bytes.len() > attachment::ATTACHMENT_MAX_BYTES {
            self.push_error("Clipboard image must be nonempty and at most 20 MiB".into());
            cx.notify();
            return;
        }
        let Some(session) = self.active.clone() else {
            self.composer
                .update(cx, |composer, cx| composer.paste_image(bytes, format, cx));
            return;
        };
        let Some(workspace) = self.active_ws_key() else {
            return;
        };
        let Some(ws) = self
            .ws(&workspace)
            .filter(|ws| ws.started && ws.inbound.is_some())
        else {
            self.push_error("Connect this chat before pasting an image".into());
            cx.notify();
            return;
        };
        let receipt = UploadReceipt {
            workspace,
            session,
            connection: ws.connection_id.clone(),
            process: ws.generation,
            navigation: self.navigation_generation,
            replacement,
        };
        self.insert_image_upload(receipt, Arc::new(bytes), format, cx);
    }

    pub(crate) fn insert_image_upload(
        &mut self,
        receipt: UploadReceipt,
        bytes: Arc<Vec<u8>>,
        format: gpui::ImageFormat,
        cx: &mut Context<Self>,
    ) {
        self.retire_stale_uploads(cx);
        let Some(ws) = self.ws_mut(&receipt.workspace) else {
            return;
        };
        let total: usize = ws
            .image_uploads
            .values()
            .map(|upload| upload.bytes.len())
            .sum();
        if ws.image_uploads.len() >= 16 || total + bytes.len() > 64 * 1024 * 1024 {
            self.push_error("Image upload staging limit reached; cancel an upload first".into());
            cx.notify();
            return;
        }
        let id = attachment::generate_upload_id();
        ws.image_uploads.insert(
            id.clone(),
            ImageUpload {
                id: id.clone(),
                receipt: receipt.clone(),
                bytes: bytes.clone(),
                format,
                prepared: None,
                state: UploadState::Preparing,
                began: false,
            },
        );
        let prepare = cx.background_executor().spawn(async move {
            attachment::chunk_payload(&bytes).map(|chunks| PreparedImage {
                chunks,
                checksum: attachment::calculate_sha256(&bytes),
            })
        });
        cx.spawn(async move |this, cx| {
            let result = prepare.await;
            let _ = this.update(cx, |state, cx| {
                state.finish_image_preparation(&receipt.workspace, &id, result, cx)
            });
        })
        .detach();
        cx.notify();
    }
}
