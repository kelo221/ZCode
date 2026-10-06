use crate::app::store::AppState;
use crate::backend::attachment_upload::{
    ImageUpload, PreparedImage, UPLOAD_ERROR, UploadState, UploadStep,
};
use crate::backend::workspace::Pending;
use crate::composer::attachment;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn handle_attachment_response(
        &mut self,
        workspace: &str,
        id: u64,
        result: &Option<Value>,
        error: &Option<Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ws(workspace).is_some_and(|ws| {
            matches!(
                ws.pending.get(&id),
                Some(Pending::AttachmentUpload { .. } | Pending::AttachmentAbort)
            )
        }) {
            return false;
        }
        if let Some(Pending::AttachmentUpload { upload, step }) =
            self.ws_mut(workspace).and_then(|ws| ws.pending.remove(&id))
        {
            self.settle_image_upload(
                workspace,
                &upload,
                step,
                if error.is_none() {
                    result.clone()
                } else {
                    None
                },
                cx,
            );
        }
        true
    }

    pub(crate) fn finish_image_preparation(
        &mut self,
        workspace: &str,
        id: &str,
        prepared: Result<PreparedImage, String>,
        cx: &mut Context<Self>,
    ) {
        self.retire_stale_uploads(cx);
        let Some(upload) = self
            .ws_mut(workspace)
            .and_then(|ws| ws.image_uploads.get_mut(id))
        else {
            return;
        };
        if !matches!(upload.state, UploadState::Preparing) {
            return;
        }
        match prepared {
            Ok(prepared) => {
                upload.prepared = Some(prepared);
                self.send_upload_step(workspace, id, UploadStep::Begin, cx);
            }
            Err(_) => self.fail_image_upload(workspace, id, cx),
        }
        cx.notify();
    }

    fn send_upload_step(
        &mut self,
        workspace: &str,
        id: &str,
        step: UploadStep,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.ws_mut(workspace) else {
            return;
        };
        let Some(upload) = ws.image_uploads.get_mut(id) else {
            return;
        };
        let Some(prepared) = &upload.prepared else {
            return;
        };
        upload.state = UploadState::Sending(step);
        let receipt = &upload.receipt;
        let (method, params) = match step {
            UploadStep::Begin => (
                "v4/attachment/begin",
                attachment::build_begin_params(
                    &receipt.connection,
                    &receipt.session,
                    id,
                    &format!("clipboard.{}", upload.format.extension()),
                    upload.format.mime_type(),
                    upload.bytes.len(),
                    prepared.chunks.len(),
                    &prepared.checksum,
                ),
            ),
            UploadStep::Chunk(index) => (
                "v4/attachment/chunk",
                attachment::build_chunk_params(
                    &receipt.connection,
                    &receipt.session,
                    id,
                    index,
                    &prepared.chunks[index],
                ),
            ),
            UploadStep::Commit => (
                "v4/attachment/commit",
                attachment::build_terminal_params(&receipt.connection, &receipt.session, id),
            ),
        };
        let request_id = ws.next_id();
        let line = json!({"id":request_id,"method":method,"params":params}).to_string();
        ws.pending.insert(
            request_id,
            Pending::AttachmentUpload {
                upload: id.into(),
                step,
            },
        );
        if line.len() > 1024 * 1024 || !ws.send_pending_line(request_id, line) {
            ws.pending.remove(&request_id);
            self.fail_image_upload(workspace, id, cx);
        }
    }

    pub(crate) fn settle_image_upload(
        &mut self,
        workspace: &str,
        id: &str,
        step: UploadStep,
        result: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        self.retire_stale_uploads(cx);
        let Some(upload) = self
            .ws_mut(workspace)
            .and_then(|ws| ws.image_uploads.get_mut(id))
        else {
            return;
        };
        if !matches!(upload.state, UploadState::Sending(current) if current == step) {
            return;
        }
        let response = result
            .ok_or(())
            .and_then(|value| upload.accept_response(step, value));
        if step == UploadStep::Begin && response.is_ok() {
            upload.began = true;
        }
        match response {
            Ok(Some(reference)) => {
                let upload = self
                    .ws_mut(workspace)
                    .unwrap()
                    .image_uploads
                    .remove(id)
                    .unwrap();
                // artifact URI 由 CLI 持有；不能按本地 paste 路径登记删除所有权。
                self.composer.update(cx, |composer, cx| {
                    composer.add_attachment(upload.attachment(reference), cx)
                });
            }
            Ok(None) => {
                let count = upload.prepared.as_ref().unwrap().chunks.len();
                let next = match step {
                    UploadStep::Begin if count > 0 => UploadStep::Chunk(0),
                    UploadStep::Chunk(index) if index + 1 < count => UploadStep::Chunk(index + 1),
                    _ => UploadStep::Commit,
                };
                self.send_upload_step(workspace, id, next, cx);
            }
            Err(()) => self.fail_image_upload(workspace, id, cx),
        }
        cx.notify();
    }

    fn abort_image_upload(&mut self, upload: &ImageUpload) {
        let Some(ws) = self.ws_mut(&upload.receipt.workspace).filter(|ws| {
            ws.generation == upload.receipt.process
                && ws.connection_id == upload.receipt.connection
                && ws.inbound.is_some()
        }) else {
            return;
        };
        let id = ws.next_id();
        let params = attachment::build_terminal_params(
            &upload.receipt.connection,
            &upload.receipt.session,
            &upload.id,
        );
        ws.pending.insert(id, Pending::AttachmentAbort);
        ws.send_pending_line(
            id,
            json!({"id":id,"method":"v4/attachment/abort","params":params}).to_string(),
        );
    }

    fn fail_image_upload(&mut self, workspace: &str, id: &str, cx: &mut Context<Self>) {
        let Some(mut upload) = self
            .ws_mut(workspace)
            .and_then(|ws| ws.image_uploads.remove(id))
        else {
            return;
        };
        if upload.began {
            self.abort_image_upload(&upload);
        }
        upload.state = UploadState::Failed;
        self.ws_mut(workspace)
            .unwrap()
            .image_uploads
            .insert(id.into(), upload);
        self.status_error(workspace, UPLOAD_ERROR);
        cx.notify();
    }

    pub(crate) fn cancel_image_upload(
        &mut self,
        workspace: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(upload) = self
            .ws_mut(workspace)
            .and_then(|ws| ws.image_uploads.remove(id))
        else {
            return;
        };
        self.ws_mut(workspace).unwrap().pending.retain(|_, pending| !matches!(pending, Pending::AttachmentUpload { upload, .. } if upload == id));
        // begin ACK 丢失不代表未 staging；同一 connection 的 abort 尽力释放，不能回滚已 commit artifact。
        if !matches!(upload.state, UploadState::Preparing) {
            self.abort_image_upload(&upload);
        }
        cx.notify();
    }

    pub(crate) fn retry_image_upload(&mut self, workspace: &str, id: &str, cx: &mut Context<Self>) {
        let Some(upload) = self.ws(workspace).and_then(|ws| ws.image_uploads.get(id)) else {
            return;
        };
        if !matches!(upload.state, UploadState::Failed)
            || !self.upload_receipt_current(&upload.receipt, cx)
        {
            return;
        }
        let receipt = upload.receipt.clone();
        let bytes = upload.bytes.clone();
        let format = upload.format;
        self.cancel_image_upload(workspace, id, cx);
        self.insert_image_upload(receipt, bytes, format, cx);
    }

    pub(crate) fn retire_stale_uploads(&mut self, cx: &mut Context<Self>) {
        let stale: Vec<_> = self
            .workspaces
            .iter()
            .flat_map(|ws| ws.image_uploads.values())
            .filter(|upload| !self.upload_receipt_current(&upload.receipt, cx))
            .map(|upload| (upload.receipt.workspace.clone(), upload.id.clone()))
            .collect();
        for (workspace, id) in stale {
            self.cancel_image_upload(&workspace, &id, cx);
        }
    }
}
