use crate::app::store::AppState;
use crate::backend::workflow_management::WorkflowMutation;
use crate::backend::workflow_management_tests::{definition, request, seed};
use gpui::{AppContext, TestAppContext};
use serde_json::json;

#[gpui::test]
fn completed_move_refreshes_only_origin_and_does_not_clear_new_selection(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "global", cx);
        s.request_workflow_confirmation(&key, WorkflowMutation::Move, cx);
        s.submit_workflow_management(&key, WorkflowMutation::Move, cx);
        let get = request(&rx);
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(definition("global")),
            None,
            cx,
        );
        let write = request(&rx);
        let (tx, other_rx) = std::sync::mpsc::channel();
        let mut other = crate::backend::workspace::WorkspaceHandle::new(
            std::env::temp_dir().join("other-workspace"),
            vec![],
        );
        other.inbound = Some(tx);
        other.started = true;
        s.active_workspace = Some(other.key.clone());
        s.workspaces.push(other);
        s.workspaces[0].saved_workflow_form.generation = "new-selection".into();
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":true,"from":"/test","to":"/destination"})),
            None,
            cx,
        );
        assert!(other_rx.try_recv().is_err());
        assert_eq!(
            s.workspaces[0].saved_workflow_form.selected.as_deref(),
            Some("test")
        );
        let global = request(&rx);
        let project = request(&rx);
        assert_eq!(global["params"]["workspace"]["workspaceKey"], key);
        assert_eq!(project["params"]["workspace"]["workspaceKey"], key);
    });
}

#[gpui::test]
fn uncertain_mutation_blocks_repetition_without_deleting_row(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = seed(s, "project", cx);
        s.request_workflow_confirmation(&key, WorkflowMutation::Delete, cx);
        s.submit_workflow_management(&key, WorkflowMutation::Delete, cx);
        let get = request(&rx);
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(definition("project")),
            None,
            cx,
        );
        let write = request(&rx);
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":true})),
            None,
            cx,
        );
        assert_eq!(
            s.workspaces[0].saved_workflow_form.selected.as_deref(),
            Some("test")
        );
        assert!(
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .needs_reload
        );
        s.request_workflow_confirmation(&key, WorkflowMutation::Delete, cx);
        s.submit_workflow_management(&key, WorkflowMutation::Delete, cx);
        assert!(rx.try_recv().is_err());
    });
}
