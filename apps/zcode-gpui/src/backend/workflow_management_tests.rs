use crate::app::store::AppState;
use crate::backend::workflow_management::{WorkflowManagementDraft, WorkflowMutation};
use crate::shared::{saved_workflows::SavedWorkflowList, workflow_definition::WorkflowDefinition};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

pub(super) fn definition(scope: &str) -> Value {
    json!({"ok":true,"name":"test","scope":scope,"path":"/test","meta":{"description":"Test","args":{"j":{"type":"json","default":null}}},"script":"return 1;"})
}
pub(super) fn seed(
    s: &mut AppState,
    scope: &str,
    cx: &mut gpui::Context<AppState>,
) -> (String, std::sync::mpsc::Receiver<String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ws = &mut s.workspaces[0];
    ws.inbound = Some(tx);
    ws.started = true;
    let key = ws.key.clone();
    ws.saved_workflows.entry(scope.into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","scope":scope,"path":"/test","description":"Test"}],"invalid":[],"dir":"/saved"}), scope).unwrap());
    ws.saved_workflow_form.scope = scope.into();
    ws.saved_workflow_form.selected = Some("test".into());
    ws.workflow_definitions
        .entry(format!("{scope}\0test"))
        .or_default()
        .finish(WorkflowDefinition::parse(&definition(scope), scope, "test").unwrap());
    s.active_workspace = Some(key.clone());
    s.active = Some("parent".into());
    s.open_workflow_management(&key, scope, "test", cx);
    (key, rx)
}
pub(super) fn request(rx: &std::sync::mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}

#[gpui::test]
fn workflow_metadata_preflight_requires_reload_and_success_keeps_new_edits(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "project", cx);
        let description = s.workspaces[0]
            .workflow_management
            .as_ref()
            .unwrap()
            .description
            .clone();
        description.update(cx, |c, _| c.set_text("Changed"));
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        let get = request(&rx);
        assert_eq!(get["method"], "workflows/get");
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        assert!(rx.try_recv().is_err());
        let mut changed = definition("project");
        changed["script"] = json!("return 2;");
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(changed.clone()),
            None,
            cx,
        );
        assert!(rx.try_recv().is_err());
        assert_eq!(description.read(cx).text(), "Changed");
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        assert!(rx.try_recv().is_err());
        s.reload_workflow_management(&key, "project", "test", cx);
        let reload = request(&rx);
        s.handle_response(
            &key,
            reload["id"].as_u64().unwrap(),
            Some(changed.clone()),
            None,
            cx,
        );
        assert_eq!(description.read(cx).text(), "Changed");
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        let get = request(&rx);
        s.handle_response(&key, get["id"].as_u64().unwrap(), Some(changed), None, cx);
        let write = request(&rx);
        assert_eq!(write["method"], "workflows/updateMeta");
        assert_eq!(write["params"]["meta"]["description"], "Changed");
        assert!(
            write["params"]["meta"]["args"]["j"]
                .get("default")
                .unwrap()
                .is_null()
        );
        assert!(write["params"].get("path").is_none());
        description.update(cx, |c, _| c.set_text("Newer edit"));
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":true,"path":"/test"})),
            None,
            cx,
        );
        assert_eq!(description.read(cx).text(), "Newer edit");
        assert!(!s.workflow_management_pending(&key));
        assert_eq!(request(&rx)["method"], "workflows/list");
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .dirty(cx)
        );
    });
}

#[gpui::test]
fn destructive_workflow_management_requires_confirmation_and_move_omits_scope(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "global", cx);
        s.submit_workflow_management(&key, WorkflowMutation::Delete, cx);
        assert!(rx.try_recv().is_err());
        s.request_workflow_confirmation(&key, WorkflowMutation::Delete, cx);
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .confirmation
                .is_some()
        );
        s.cancel_workflow_confirmation(&key, cx);
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .confirmation
                .is_none()
        );
        assert!(!s.workflow_management_pending(&key));
        s.request_workflow_confirmation(&key, WorkflowMutation::Move, cx);
        s.submit_workflow_management(&key, WorkflowMutation::Move, cx);
        let get = request(&rx);
        assert_eq!(get["params"]["scope"], "global");
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(definition("global")),
            None,
            cx,
        );
        let write = request(&rx);
        assert_eq!(write["method"], "workflows/move");
        assert!(write["params"].get("scope").is_none());
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":false,"reason":"target_exists","path":"/target"})),
            None,
            cx,
        );
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .needs_reload
        );
        assert_eq!(
            s.workspaces[0].saved_workflow_form.selected.as_deref(),
            Some("test")
        );
        assert!(rx.try_recv().is_err());
    });
}

#[gpui::test]
fn workflow_management_reset_retains_edits_and_blocks_uncertain_write(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "project", cx);
        let input = s.workspaces[0]
            .workflow_management
            .as_ref()
            .unwrap()
            .description
            .clone();
        input.update(cx, |c, _| c.set_text("Kept"));
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        let _ = request(&rx);
        s.workspaces[0].invalidate_connection();
        assert!(!s.workflow_management_pending(&key));
        assert_eq!(input.read(cx).text(), "Kept");
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .needs_reload
        );
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        assert!(rx.try_recv().is_err());
    });
}

#[gpui::test]
fn old_management_error_cannot_annotate_new_form(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "project", cx);
        s.workspaces[0]
            .workflow_management
            .as_ref()
            .unwrap()
            .description
            .update(cx, |c, _| c.set_text("Changed"));
        s.submit_workflow_management(&key, WorkflowMutation::Metadata, cx);
        let get = request(&rx);
        let replacement = WorkflowManagementDraft::new(
            WorkflowDefinition::parse(&definition("project"), "project", "test").unwrap(),
            cx,
        );
        s.workspaces[0].workflow_management = Some(replacement);
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"token=secret"})),
            cx,
        );
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .error
                .is_none()
        );
    });
}
