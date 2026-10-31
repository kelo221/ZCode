//! Phone uploads go to the desktop's `gcode/stage/*` endpoint, including from
//! a new-task draft that has no session yet.

use crate::app::store::AppState;
use crate::backend::attachment_upload::PreparedImage;
use crate::composer::attachment::{calculate_sha256, chunk_payload};
use crate::remote::staging::StagedUploads;
use gpui::{AppContext, TestAppContext};
use serde_json::Value;
use std::sync::mpsc::channel;

#[gpui::test]
fn phone_draft_file_upload_stages_on_the_desktop_and_attaches_its_path(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    let (tx, rx) = channel();
    let key = state.update(cx, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = None;
        s.draft = true;
        s.phone_client.enabled = true;
        key
    });
    let bytes = b"fn main() {}\n".repeat(40_000);
    state.update(cx, |s, cx| {
        s.start_file_upload("main.rs".into(), "text/x-rust".into(), bytes.clone(), cx);
        let id = s
            .ws(&key)
            .unwrap()
            .image_uploads
            .keys()
            .next()
            .unwrap()
            .clone();
        let prepared = PreparedImage {
            chunks: chunk_payload(&bytes).unwrap(),
            checksum: calculate_sha256(&bytes),
        };
        s.finish_image_preparation(&key, &id, Ok(prepared), cx);
    });

    // The desktop answers every request; nothing reaches a CLI.
    let mut desktop = StagedUploads::default();
    let mut methods = Vec::new();
    while let Ok(line) = rx.try_recv() {
        let request: Value = serde_json::from_str(&line).unwrap();
        methods.push(request["method"].as_str().unwrap().to_string());
        assert_eq!(request["params"]["sessionId"], "");
        let reply: Value = serde_json::from_str(&desktop.intercept(&line).unwrap()).unwrap();
        assert!(reply["error"].is_null(), "{reply}");
        state.update(cx, |s, cx| {
            s.handle_response(
                &key,
                request["id"].as_u64().unwrap(),
                Some(reply["result"].clone()),
                None,
                cx,
            )
        });
    }
    assert_eq!(methods.first().unwrap(), "gcode/stage/begin");
    assert_eq!(methods.last().unwrap(), "gcode/stage/commit");

    state.update(cx, |s, cx| {
        assert!(s.ws(&key).unwrap().image_uploads.is_empty());
        let attachments = s.composer.read(cx).attachments().to_vec();
        assert_eq!(attachments.len(), 1);
        let attached = &attachments[0];
        assert_eq!(attached.file_name, "main.rs");
        assert_eq!(attached.mime, "text/x-rust");
        let path = std::path::PathBuf::from(&attached.reference);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        crate::shared::temp_attachments::delete_owned(&path);
    });
}
