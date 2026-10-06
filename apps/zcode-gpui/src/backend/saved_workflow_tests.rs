use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::saved_workflows::SavedWorkflowList;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn ready(s: &mut AppState) -> (String, std::sync::mpsc::Receiver<String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let key = s.workspaces[0].key.clone();
    s.workspaces[0].inbound = Some(tx);
    s.workspaces[0].started = true;
    s.active_workspace = Some(key.clone());
    s.active = Some("parent".into());
    s.draft = false;
    let list = SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","description":"Test","scope":"project","path":"/isolated/test.ts","args":{"count":{"type":"number","required":true}}}],"invalid":[],"dir":"/isolated"}), "project").unwrap();
    s.workspaces[0]
        .saved_workflows
        .entry("project".into())
        .or_default()
        .finish(list);
    (key, rx)
}
fn read(rx: &std::sync::mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}

#[gpui::test]
fn saved_workflow_launch_creates_before_start_and_preserves_parent(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        s.select_saved_workflow(&key, "project", "test", cx);
        let input = s.workspaces[0].saved_workflow_form.inputs["count"].clone();
        input.update(cx, |c, _| c.set_text("4"));
        s.launch_saved_workflow(&key, "project", "test", cx);
        s.launch_saved_workflow(&key, "project", "test", cx);
        let create = read(&rx);
        assert_eq!(create["params"]["type"], "createSession");
        assert_eq!(create["params"]["sessionId"], Value::Null);
        assert_eq!(create["params"]["payload"], json!({"workspaceId":key}));
        assert!(rx.try_recv().is_err());
        s.handle_response(&key, create["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"createSession","sessionId":"target"}})), None, cx);
        assert_eq!(s.active.as_deref(), Some("parent"));
        let start = read(&rx);
        assert_eq!(start["params"]["type"], "startSavedWorkflow");
        assert_eq!(start["params"]["sessionId"], "target");
        assert_eq!(start["params"]["payload"]["args"], json!({"count":4}));
        assert!(start["params"].get("baseRevision").is_none());
        s.handle_response(&key, start["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"startSavedWorkflow","runId":"run","toolCallId":"launch"}})), None, cx);
        assert_eq!(s.active.as_deref(), Some("target"));
        assert_eq!(s.session_drafts["parent"], "parent draft");
        assert_eq!(input.read(cx).text(), "4");
    });
}

#[gpui::test]
fn saved_workflow_rejection_cleans_only_new_target_and_unknown_ack_never_deletes(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.select_saved_workflow(&key, "project", "test", cx);
        s.workspaces[0].saved_workflow_form.inputs["count"].update(cx, |c, _| c.set_text("1"));
        for (ack, cleanup) in [(json!({"status":"failed","reasonCode":"fault.command.capabilityUnsupported"}), true), (json!({"status":"accepted","result":{}}), false)] {
            s.launch_saved_workflow(&key, "project", "test", cx);
            let create = read(&rx);
            s.handle_response(&key, create["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"createSession","sessionId":"target"}})), None, cx);
            let start = read(&rx);
            s.handle_response(&key, start["id"].as_u64().unwrap(), Some(ack), None, cx);
            assert_eq!(s.active.as_deref(), Some("parent"));
            assert!(s.workspaces[0].saved_workflow_form.error.is_some());
            if cleanup {
                let delete = read(&rx);
                assert_eq!(delete["params"]["type"], "deleteSession");
                assert_eq!(delete["params"]["sessionId"], "target");
                s.handle_response(&key, delete["id"].as_u64().unwrap(), Some(json!({"status":"accepted"})), None, cx);
            } else { assert!(rx.try_recv().is_err()); }
        }
    });
}

#[gpui::test]
fn saved_workflow_late_launch_keeps_new_navigation_and_lists_are_scope_local(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.fetch_saved_workflows("global", true, cx);
        s.fetch_saved_workflows("global", true, cx);
        let request = read(&rx);
        assert_eq!(request["params"]["scope"], "global");
        assert!(rx.try_recv().is_err());
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"workflows":[],"invalid":[],"dir":"/global"})), None, cx);
        assert_eq!(s.workspaces[0].saved_workflows["project"].value.as_ref().unwrap().workflows.len(), 1);
        s.select_saved_workflow(&key, "project", "test", cx);
        s.workspaces[0].saved_workflow_form.inputs["count"].update(cx, |c, _| c.set_text("2"));
        s.launch_saved_workflow(&key, "project", "test", cx);
        let create = read(&rx);
        s.navigation_generation += 1;
        s.active = Some("newer".into());
        s.handle_response(&key, create["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"createSession","sessionId":"target"}})), None, cx);
        let start = read(&rx);
        s.handle_response(&key, start["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"startSavedWorkflow","runId":"run","toolCallId":"launch"}})), None, cx);
        assert_eq!(s.active.as_deref(), Some("newer"));
        assert!(rx.try_recv().is_err());
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].saved_workflows.is_empty());
        assert_eq!(s.workspaces[0].saved_workflow_form.inputs["count"].read(cx).text(), "2");
        assert!(!s.workspaces[0].pending.values().any(|p| matches!(p, Pending::SavedWorkflowCreate(_) | Pending::SavedWorkflowStart {..})));
    });
}
