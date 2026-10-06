use crate::{app::store::AppState, conversation::workflows_types::WorkflowRunState};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn setup(s: &mut AppState) -> (String, std::sync::mpsc::Receiver<String>) {
    let key = s.workspaces[0].key.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    s.workspaces[0].inbound = Some(tx);
    s.workspaces[0].started = true;
    s.active_workspace = Some(key.clone());
    s.active = Some("parent".into());
    s.conversations
        .entry("parent".into())
        .or_default()
        .workflow_runs
        .runs = vec![
        WorkflowRunState {
            run_id: "run".into(),
            tool_call_id: Some("tool".into()),
            status: "errored".into(),
            ..Default::default()
        },
        WorkflowRunState {
            run_id: "other".into(),
            tool_call_id: Some("other-tool".into()),
            status: "stopped".into(),
            ..Default::default()
        },
    ];
    assert!(s.focus_workflow(&key, "parent", "run", "tool"));
    (key, rx)
}
fn request(rx: &std::sync::mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}
fn metadata() -> Value {
    json!({"artifacts":[{"id":"report","kind":"markdown","title":"Report","version":1,
        "versions":[{"version":1,"publishedAt":1,"uri":"zcode-artifact://parent/private"}],
        "itemCount":0,"primary":true}]})
}

#[gpui::test]
fn artifact_query_uses_parent_run_once_and_refresh_retains_safe_metadata(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = setup(s);
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        s.ensure_workflow_artifacts(false, cx);
        s.ensure_workflow_artifacts(false, cx);
        let req = request(&rx);
        assert_eq!(req["method"], "v4/conversation/workflowRunArtifacts");
        assert_eq!(req["params"], json!({"sessionId":"parent","runId":"run"}));
        assert!(rx.try_recv().is_err());
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(metadata()),
            None,
            cx,
        );
        assert_eq!(
            s.active_workflow_artifacts()
                .unwrap()
                .query
                .value
                .as_ref()
                .unwrap()[0]
                .title
                .as_deref(),
            Some("Report")
        );
        let origin = s.workflow_focus().unwrap().clone();
        s.refresh_workflow_artifacts(&origin, cx);
        let refresh = request(&rx);
        s.handle_response(
            &key,
            refresh["id"].as_u64().unwrap(),
            None,
            Some(json!({"code":-1,"message":"secret-sentinel"})),
            cx,
        );
        let query = &s.active_workflow_artifacts().unwrap().query;
        assert_eq!(
            query.error.as_deref(),
            Some(crate::shared::workflow_artifacts::ARTIFACT_ERROR)
        );
        assert_eq!(query.value.as_ref().unwrap().len(), 1);
        assert_eq!(s.composer.read(cx).text(), "parent draft");
        assert!(!format!("{:?}", s.log).contains("secret-sentinel"));
        s.refresh_workflow_artifacts(&origin, cx);
        let req = request(&rx);
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(json!({"artifacts":[]})),
            None,
            cx,
        );
        assert!(
            s.active_workflow_artifacts()
                .unwrap()
                .query
                .value
                .as_ref()
                .unwrap()
                .is_empty()
        );
    });
}

#[gpui::test]
fn artifact_query_distinguishes_unsupported_malformed_and_enqueue_failure(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = setup(s);
        s.ensure_workflow_artifacts(false, cx);
        let req = request(&rx);
        let unsupported = json!({"code":-32601,"message":"secret-sentinel"});
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            None,
            Some(unsupported),
            cx,
        );
        assert_eq!(
            s.active_workflow_artifacts()
                .unwrap()
                .query
                .error
                .as_deref(),
            Some(crate::shared::workflow_artifacts::ARTIFACT_UNAVAILABLE)
        );
        let origin = s.workflow_focus().unwrap().clone();
        s.refresh_workflow_artifacts(&origin, cx);
        let req = request(&rx);
        s.handle_response(&key, req["id"].as_u64().unwrap(), None,
            Some(json!({"code":-32603,"data":{"name":"V4CapabilityUnsupportedError","stack":"secret-sentinel"},"message":"secret-sentinel"})), cx);
        assert_eq!(s.active_workflow_artifacts().unwrap().query.error.as_deref(), Some(crate::shared::workflow_artifacts::ARTIFACT_UNAVAILABLE));
        s.refresh_workflow_artifacts(&origin, cx);
        let req = request(&rx);
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(json!({"artifacts":[{"id":"wrong"}]})),
            None,
            cx,
        );
        assert_eq!(
            s.active_workflow_artifacts()
                .unwrap()
                .query
                .error
                .as_deref(),
            Some(crate::shared::workflow_artifacts::ARTIFACT_ERROR)
        );
        assert!(s.active_workflow_artifacts().unwrap().query.value.is_none());
        drop(rx);
        s.refresh_workflow_artifacts(&origin, cx);
        assert!(!s.active_workflow_artifacts().unwrap().query.loading);
        assert!(s.workspaces[0].pending.is_empty());
    });
}

#[gpui::test]
fn artifact_query_reentry_parent_connection_and_removed_run_reject_late_results(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = setup(s);
        s.ensure_workflow_artifacts(false, cx);
        let old = request(&rx);
        let old_origin = s.workflow_focus().unwrap().clone();
        s.all_workflow_runs();
        assert!(s.active_workflow_artifacts().is_none());
        assert!(s.focus_workflow(&key, "parent", "run", "tool"));
        s.ensure_workflow_artifacts(false, cx);
        let new = request(&rx);
        s.refresh_workflow_artifacts(&old_origin, cx);
        assert!(rx.try_recv().is_err());
        s.handle_response(
            &key,
            old["id"].as_u64().unwrap(),
            Some(metadata()),
            None,
            cx,
        );
        assert!(s.active_workflow_artifacts().unwrap().query.value.is_none());
        s.handle_response(
            &key,
            new["id"].as_u64().unwrap(),
            Some(metadata()),
            None,
            cx,
        );
        assert!(s.active_workflow_artifacts().unwrap().query.value.is_some());
        s.ensure_workflow_artifacts(true, cx);
        let req = request(&rx);
        s.active = Some("different-parent".into());
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(json!({"artifacts":[]})),
            None,
            cx,
        );
        assert!(s.active_workflow_artifacts().is_none());
        s.active = Some("parent".into());
        assert!(s.focus_workflow(&key, "parent", "run", "tool"));
        s.ensure_workflow_artifacts(false, cx);
        let req = request(&rx);
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].workflow_artifacts.is_empty());
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(metadata()),
            None,
            cx,
        );
        assert!(s.workspaces[0].workflow_artifacts.is_empty());
        assert!(s.focus_workflow(&key, "parent", "run", "tool"));
        s.ensure_workflow_artifacts(false, cx);
        let req = request(&rx);
        s.conversations
            .get_mut("parent")
            .unwrap()
            .workflow_runs
            .runs
            .clear();
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(metadata()),
            None,
            cx,
        );
        assert!(s.active_workflow_artifacts().is_none());
    });
}
