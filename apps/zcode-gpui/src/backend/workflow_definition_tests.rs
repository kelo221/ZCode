use crate::app::store::AppState;
use crate::shared::saved_workflows::SavedWorkflowList;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn definition_query_is_scoped_checked_and_retained_on_failed_refresh(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.workspaces[0].saved_workflows.entry("project".into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","description":"Test","scope":"project","path":"/test"}],"invalid":[],"dir":"/dir"}), "project").unwrap());
        s.select_saved_workflow(&key, "project", "test", cx);
        s.inspect_workflow_definition(&key, "global", "test", false, cx);
        assert!(rx.try_recv().is_err());
        s.inspect_workflow_definition(&key, "project", "test", false, cx);
        s.inspect_workflow_definition(&key, "project", "test", false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "workflows/get");
        assert_eq!(request["params"]["workspace"]["workspaceKey"], key);
        assert_eq!(request["params"]["name"], "test");
        assert!(request["params"].get("path").is_none());
        assert!(rx.try_recv().is_err());
        s.active = Some("newer".into());
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"ok":true,"name":"test","scope":"project","path":"/test","meta":{"description":"Definition"},"script":"return 1;"})), None, cx);
        assert_eq!(s.active.as_deref(), Some("newer"));
        assert_eq!(s.workspaces[0].workflow_definitions["project\0test"].value.as_ref().unwrap().script, "return 1;");
        s.inspect_workflow_definition(&key, "project", "test", true, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"ok":false,"reason":"read_error","detail":"token=secret"})), None, cx);
        let query = &s.workspaces[0].workflow_definitions["project\0test"];
        assert!(!query.loading);
        assert!(!query.error.as_ref().unwrap().contains("secret"));
        assert_eq!(query.value.as_ref().unwrap().script, "return 1;");
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].workflow_definitions.is_empty());
        assert_eq!(s.workspaces[0].saved_workflow_form.selected.as_deref(), Some("test"));
    });
}
