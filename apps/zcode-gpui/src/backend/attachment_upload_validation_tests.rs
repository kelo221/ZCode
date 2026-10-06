use crate::app::store::AppState;
use crate::backend::attachment_upload::{
    ImageUpload, PreparedImage, UploadReceipt, UploadState, UploadStep,
};
use crate::composer::attachment::ATTACHMENT_MAX_BYTES;
use gpui::{AppContext, ImageFormat, TestAppContext};
use serde_json::json;
use std::sync::Arc;

fn upload() -> ImageUpload {
    ImageUpload {
        id: "upload-test".into(),
        receipt: UploadReceipt {
            workspace: "workspace".into(),
            session: "parent".into(),
            connection: "conn".into(),
            process: 0,
            navigation: 0,
            replacement: 0,
        },
        bytes: Arc::new(vec![1]),
        format: ImageFormat::Png,
        prepared: Some(PreparedImage {
            chunks: vec!["AQ==".into()],
            checksum: "checksum".into(),
        }),
        state: UploadState::Sending(UploadStep::Begin),
        began: false,
    }
}

#[test]
fn image_upload_strict_results_reject_wrong_ids_extra_fields_and_path_refs() {
    let upload = upload();
    for value in [
        json!({"state":"staging","uploadId":"wrong","nextChunkIndex":0}),
        json!({"state":"staging","uploadId":"upload-test","nextChunkIndex":0,"extra":true}),
        json!({"state":"committed","uploadId":"upload-test","nextChunkIndex":0,"ref":"zcode-artifact://parent/image"}),
    ] {
        assert!(upload.accept_response(UploadStep::Begin, value).is_err());
    }
    for value in [
        json!({"uploadId":"wrong","nextChunkIndex":1}),
        json!({"uploadId":"upload-test","nextChunkIndex":2}),
        json!({"uploadId":"upload-test","nextChunkIndex":1,"extra":true}),
    ] {
        assert!(upload.accept_response(UploadStep::Chunk(0), value).is_err());
    }
    for value in [
        json!({"ref":"local-path"}),
        json!({"ref":" https://example.test/image "}),
        json!({"ref":"zcode-artifact://parent/image","extra":true}),
        json!({"ref":""}),
    ] {
        assert!(upload.accept_response(UploadStep::Commit, value).is_err());
    }
    assert_eq!(
        upload.accept_response(
            UploadStep::Commit,
            json!({"ref":"zcode-artifact://parent/image"})
        ),
        Ok(Some("zcode-artifact://parent/image".into()))
    );
}

#[gpui::test]
fn image_upload_limits_read_only_and_replacement_are_checked_before_adoption(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.draft = false;
        s.start_image_upload(vec![0; ATTACHMENT_MAX_BYTES + 1], ImageFormat::Png, 0, cx);
        s.viewing_child = Some("child".into());
        s.start_image_upload(vec![1], ImageFormat::Png, 0, cx);
        s.viewing_child = None;
        assert!(s.ws(&key).unwrap().image_uploads.is_empty());
        s.start_image_upload(vec![1], ImageFormat::Png, 0, cx);
        let id = s
            .ws(&key)
            .unwrap()
            .image_uploads
            .keys()
            .next()
            .unwrap()
            .clone();
        s.composer.update(cx, |c, _| c.set_text("replacement"));
        s.finish_image_preparation(
            &key,
            &id,
            Ok(PreparedImage {
                chunks: vec!["AQ==".into()],
                checksum: "checksum".into(),
            }),
            cx,
        );
        assert!(s.ws(&key).unwrap().image_uploads.is_empty());
        assert!(s.composer.read(cx).attachments.is_empty());
        assert_eq!(s.composer.read(cx).text(), "replacement");
        assert!(rx.try_recv().is_err());
    });
}
