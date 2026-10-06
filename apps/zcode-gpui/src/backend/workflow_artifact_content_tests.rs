use crate::{app::store::AppState, conversation::workflows_types::WorkflowRunState};
use base64::{Engine, prelude::BASE64_STANDARD};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn setup(
    s: &mut AppState,
    cx: &mut gpui::Context<AppState>,
) -> (String, std::sync::mpsc::Receiver<String>) {
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
        .runs = vec![WorkflowRunState {
        run_id: "run".into(),
        tool_call_id: Some("tool".into()),
        status: "completed".into(),
        ..Default::default()
    }];
    assert!(s.focus_workflow(&key, "parent", "run", "tool"));
    s.ensure_workflow_artifacts(false, cx);
    let req = request(&rx);
    s.handle_response(&key, req["id"].as_u64().unwrap(), Some(json!({"artifacts":[
        {"id":"report","kind":"markdown","contentType":"text/markdown","version":2,"versions":[{"version":2,"publishedAt":1}],"itemCount":0},
        {"id":"other","kind":"markdown","contentType":"text/markdown","version":1,"versions":[{"version":1,"publishedAt":1}],"itemCount":0},
        {"id":"file","kind":"file","contentType":"text/markdown","version":1,"versions":[{"version":1,"publishedAt":1}],"itemCount":0}
    ]})), None, cx);
    (key, rx)
}
fn request(rx: &std::sync::mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}
fn body() -> Value {
    json!({"dataBase64":BASE64_STANDARD.encode("# Report"),"mediaType":"text/markdown","totalBytes":8,"nextOffset":null})
}

#[gpui::test]
fn markdown_content_view_is_exact_version_explicit_retry_and_close(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = setup(s, cx);
        let origin = s.workflow_focus().unwrap().clone();
        s.view_workflow_markdown(&origin, "file", 1, cx);
        s.view_workflow_markdown(&origin, "report", 1, cx);
        assert!(rx.try_recv().is_err());
        s.view_workflow_markdown(&origin, "report", 2, cx);
        s.view_workflow_markdown(&origin, "report", 2, cx);
        let req = request(&rx);
        assert_eq!(req["method"], "v4/conversation/workflowRunArtifactRead");
        assert_eq!(req["params"], json!({"sessionId":"parent","runId":"run","artifactId":"report","version":2,"offset":0,"limit":262144}));
        assert!(rx.try_recv().is_err());
        s.handle_response(&key, req["id"].as_u64().unwrap(), None, Some(json!({"message":"secret-sentinel"})), cx);
        let content = s.active_workflow_artifacts().unwrap().content.as_ref().unwrap();
        assert!(content.query.value.is_none()); assert!(content.query.error.is_some());
        s.retry_workflow_markdown(&origin, "report", 2, cx);
        let req = request(&rx);
        s.handle_response(&key, req["id"].as_u64().unwrap(), Some(body()), None, cx);
        assert_eq!(s.active_workflow_artifacts().unwrap().content.as_ref().unwrap().query.value.as_deref(), Some("# Report"));
        s.close_workflow_markdown(&origin, "report", 2, cx);
        assert!(s.active_workflow_artifacts().unwrap().content.is_none());
        assert!(!format!("{:?}", s.log).contains("secret-sentinel"));
    });
}

#[gpui::test]
fn markdown_content_reselection_refresh_and_connection_discard_late_bytes(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = setup(s, cx);
        let origin = s.workflow_focus().unwrap().clone();
        s.view_workflow_markdown(&origin, "report", 2, cx);
        let old = request(&rx);
        s.view_workflow_markdown(&origin, "other", 1, cx);
        let current = request(&rx);
        s.handle_response(&key, old["id"].as_u64().unwrap(), Some(body()), None, cx);
        assert!(
            s.active_workflow_artifacts()
                .unwrap()
                .content
                .as_ref()
                .unwrap()
                .query
                .value
                .is_none()
        );
        s.close_workflow_markdown(&origin, "report", 2, cx);
        assert!(s.active_workflow_artifacts().unwrap().content.is_some());
        s.refresh_workflow_artifacts(&origin, cx);
        let _ = request(&rx);
        s.handle_response(
            &key,
            current["id"].as_u64().unwrap(),
            Some(body()),
            None,
            cx,
        );
        assert!(s.active_workflow_artifacts().unwrap().content.is_none());
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].workflow_artifacts.is_empty());
    });
}
