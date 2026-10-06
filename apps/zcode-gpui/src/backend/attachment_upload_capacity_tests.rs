use crate::app::store::AppState;
use crate::backend::attachment_upload::{ImageUpload, UploadReceipt, UploadState};
use gpui::{AppContext, ImageFormat, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn image_upload_capacity_and_stale_retry_keep_the_current_owner_bounded(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    state.update(cx, |s, cx| {
        let ws = &mut s.workspaces[0];
        ws.started = true;
        ws.inbound = Some(tx);
        let key = ws.key.clone();
        let receipt = UploadReceipt {
            workspace: key.clone(),
            session: "parent".into(),
            connection: ws.connection_id.clone(),
            process: ws.generation,
            navigation: 0,
            replacement: 0,
        };
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.draft = false;
        for index in 0..16 {
            let id = format!("upload-{index}");
            s.ws_mut(&key).unwrap().image_uploads.insert(
                id.clone(),
                ImageUpload {
                    id,
                    receipt: receipt.clone(),
                    bytes: Arc::new(vec![1]),
                    format: ImageFormat::Png,
                    prepared: None,
                    state: UploadState::Failed,
                    began: false,
                },
            );
        }
        s.insert_image_upload(receipt.clone(), Arc::new(vec![2]), ImageFormat::Png, cx);
        assert_eq!(s.ws(&key).unwrap().image_uploads.len(), 16);
        s.ws_mut(&key).unwrap().image_uploads.clear();
        let bytes = Arc::new(vec![0; 16 * 1024 * 1024]);
        for index in 0..4 {
            let id = format!("upload-large-{index}");
            s.ws_mut(&key).unwrap().image_uploads.insert(
                id.clone(),
                ImageUpload {
                    id,
                    receipt: receipt.clone(),
                    bytes: bytes.clone(),
                    format: ImageFormat::Png,
                    prepared: None,
                    state: UploadState::Failed,
                    began: false,
                },
            );
        }
        s.insert_image_upload(receipt, Arc::new(vec![2]), ImageFormat::Png, cx);
        assert_eq!(s.ws(&key).unwrap().image_uploads.len(), 4);
        s.active = Some("other-parent".into());
        s.retry_image_upload(&key, "upload-large-0", cx);
        assert_eq!(s.ws(&key).unwrap().image_uploads.len(), 4);
        assert!(rx.try_recv().is_err());
        s.ws_mut(&key).unwrap().image_uploads.clear();
    });
}
