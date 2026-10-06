use crate::app::store::AppState;
use crate::backend::submission::Submission;
use crate::backend::workspace::Pending;
use crate::composer::attachment::AttachmentRef;
use crate::conversation::msg_actions::ComposerIntent;
use gpui::{AppContext, TestAppContext};
use serde_json::json;

fn submission(state: &AppState, workspace: &str, draft_key: &str) -> Submission {
    Submission {
        workspace: workspace.into(),
        draft_key: draft_key.into(),
        navigation_generation: state.navigation_generation,
        text: "original".into(),
        config: serde_json::Value::Null,
        attachments: vec![AttachmentRef {
            reference: "attachment-ref".into(),
            file_name: "test.txt".into(),
            mime: "text/plain".into(),
            bytes: 1,
            preview_ref: None,
        }],
    }
}

#[gpui::test]
fn rejection_does_not_overwrite_edit_intent(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        state.active = Some("session".into());
        state.composer_intent = ComposerIntent::Rename {
            ws_key: key.clone(),
            sid: "session".into(),
        };
        state.composer.update(cx, |c, _| c.set_text("new title"));
        state.recover_submission(submission(state, &key, "session"), cx);
        assert_eq!(state.composer.read(cx).text(), "new title");
        assert!(state.composer.read(cx).attachments().is_empty());
        state.cancel_intent(cx);
        assert_eq!(state.composer.read(cx).text(), "original");
        assert_eq!(state.composer.read(cx).attachments().len(), 1);
    });
}

#[gpui::test]
fn inactive_rejection_restores_only_origin_draft(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        state.active = Some("other".into());
        state.composer.update(cx, |c, _| c.set_text("keep current"));
        state
            .session_drafts
            .insert("origin".into(), "new origin draft".into());
        state.recover_submission(submission(state, &key, "origin"), cx);
        assert_eq!(state.composer.read(cx).text(), "keep current");
        state.restore_draft("origin", cx);
        assert_eq!(state.composer.read(cx).text(), "original\nnew origin draft");
        assert_eq!(state.composer.read(cx).attachments().len(), 1);
    });
}

#[gpui::test]
fn accepted_create_does_not_steal_newer_draft_navigation(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        let draft_key = format!("draft:{key}");
        let pending = submission(state, &key, &draft_key);
        state.workspaces[0]
            .pending
            .insert(1, Pending::CreateSession(pending));
        state.new_chat(cx);
        state.composer.update(cx, |c, _| c.set_text("new context"));
        state.handle_response(
            &key,
            1,
            Some(json!({"status":"accepted","result":{"sessionId":"created"}})),
            None,
            cx,
        );
        assert!(state.active.is_none());
        assert_eq!(state.composer.read(cx).text(), "new context");
    });
}

#[gpui::test]
fn malformed_ack_does_not_offer_duplicate_submission(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = state.workspaces[0].key.clone();
        state.active_workspace = Some(key.clone());
        state.active = Some("session".into());
        let pending = submission(state, &key, "session");
        state.workspaces[0]
            .pending
            .insert(1, Pending::SendText(pending));
        state.composer.update(cx, |c, _| c.set_text("new draft"));
        state.handle_response(&key, 1, Some(json!({})), None, cx);
        assert_eq!(state.composer.read(cx).text(), "new draft");
        assert!(state.errors.iter().any(|e| e.contains("unknown")));
    });
}
