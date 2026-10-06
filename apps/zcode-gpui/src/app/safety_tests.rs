use crate::app::store::AppState;
use crate::backend::workspace::{Pending, WorkspaceHandle};
use gpui::{AppContext, TestAppContext};
use serde_json::json;

#[gpui::test]
fn failed_enqueue_restores_submission_and_removes_pending(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        state.workspaces[0].started = true;
        state.workspaces[0].pumping = true;
        let (tx, rx) = std::sync::mpsc::channel();
        state.workspaces[0].inbound = Some(tx);
        drop(rx);
        let temp = std::env::temp_dir().join("zcode-gpui-paste-enqueue-test.png");
        let attachment = crate::composer::attachment::AttachmentRef {
            reference: temp.to_string_lossy().into_owned(),
            file_name: "test.png".into(),
            mime: "image/png".into(),
            bytes: 1,
            preview_ref: None,
        };
        state.composer.update(cx, |composer, cx| {
            composer.set_text("unsent");
            composer.add_owned_attachment(attachment.clone(), cx);
        });
        state.submit_composer(cx);
        assert_eq!(state.composer.read(cx).text(), "unsent");
        assert_eq!(state.composer.read(cx).attachments(), &[attachment]);
        assert_eq!(state.composer.read(cx).temp_owned, [temp]);
        assert!(state.retired_temp_files.is_empty());
        assert!(state.workspaces[0].pending.is_empty());
        assert!(!state.errors.is_empty());
    });
}

#[gpui::test]
fn failed_query_enqueue_does_not_leave_pending(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        state.active_workspace = Some(state.workspaces[0].key.clone());
        state.workspaces[0].started = true;
        let (tx, rx) = std::sync::mpsc::channel();
        state.workspaces[0].inbound = Some(tx);
        drop(rx);
        state.fetch_usage_stats("week", cx);
        state.fetch_mcp_servers(cx);
        state.fetch_plugins_overview(cx);
        assert!(state.workspaces[0].pending.is_empty());
    });
}

#[gpui::test]
fn rejected_send_ack_restores_original_without_overwriting_new_text(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        state.active = Some("session".into());
        let submission = crate::backend::submission::Submission {
            workspace: key.clone(),
            draft_key: "session".into(),
            navigation_generation: state.navigation_generation,
            text: "original".into(),
            config: serde_json::Value::Null,
            attachments: vec![],
        };
        state.workspaces[0]
            .pending
            .insert(1, Pending::SendText(submission));
        state
            .composer
            .update(cx, |composer, _| composer.set_text("new draft"));
        state.handle_response(&key, 1, Some(json!({"status":"rejected"})), None, cx);
        assert_eq!(state.composer.read(cx).text(), "original\nnew draft");
    });
}

#[gpui::test]
fn background_response_does_not_poison_active_workspace(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let a = state.workspaces[0].key.clone();
        state.workspaces[0].status = "connected".into();
        state.active_workspace = Some(a);
        let mut b = WorkspaceHandle::new(std::env::temp_dir().join("background"), vec![]);
        b.pending.insert(1, Pending::Stop);
        let key = b.key.clone();
        state.workspaces.push(b);
        state.handle_response(
            &key,
            1,
            None,
            Some(json!({"message":"Authorization: Bearer sentinel-secret"})),
            cx,
        );
        assert_eq!(state.workspaces[0].status, "connected");
        assert!(!state.workspaces[1].status.contains("sentinel-secret"));
        assert!(state.errors.iter().all(|s| !s.contains("sentinel-secret")));
        assert!(state.log.iter().all(|s| !s.contains("sentinel-secret")));
    });
}
