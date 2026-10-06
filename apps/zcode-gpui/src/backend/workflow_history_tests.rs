use crate::app::store::AppState;
use crate::shared::{saved_workflows::SavedWorkflowList, workflow_history::WorkflowHistory};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn history_query_is_scoped_and_open_chat_preserves_draft(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone()); s.active = Some("parent".into());
        s.composer.update(cx, |c, _| c.set_text("draft"));
        s.workspaces[0].saved_workflows.entry("project".into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","description":"Test","scope":"project","path":"/test"}],"invalid":[],"dir":"/dir"}), "project").unwrap());
        s.select_saved_workflow(&key, "project", "test", cx);
        s.fetch_workflow_history(&key, "project", "test", false, cx);
        s.fetch_workflow_history(&key, "project", "test", false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "workflows/runs");
        assert_eq!(request["params"]["limit"], 50);
        assert!(rx.try_recv().is_err());
        let result = json!({"runs":[{"runId":"run","name":"test","status":"completed","createdAt":1,"updatedAt":2,"spentTokens":0,"parentSessionId":"target","toolCallId":"call"}]});
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(result.clone()), None, cx);
        s.open_workflow_history_chat(&key, "project", "test", "other", cx);
        assert_eq!(s.active.as_deref(), Some("parent"));
        s.open_workflow_history_chat(&key, "project", "test", "run", cx);
        assert_eq!(s.active.as_deref(), Some("target"));
        assert_eq!(s.session_drafts["parent"], "draft");
        s.workspaces[0].workflow_histories.entry("global\0test".into()).or_default().finish(WorkflowHistory::parse(&result, "test").unwrap());
        assert!(s.workflow_history_parent(&key, "global", "test", "run").is_none());
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].workflow_histories.is_empty());
    });
}
