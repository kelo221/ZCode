use crate::app::store::AppState;
use crate::conversation::{model::ConversationState, workflows_types::WorkflowRunState};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn settings_confirmation_rebases_without_losing_newer_edits(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        let mut conversation = ConversationState::default();
        conversation.workflow_runs.runs.push(WorkflowRunState {
            run_id: "run".into(), status: "running".into(), concurrency_ceiling: Some(8),
            ..Default::default()
        });
        s.conversations.insert("parent".into(), conversation);
        s.open_workflow_settings(&key, "parent", "run", cx);
        let field = s.workspaces[0].workflow_settings["parent\0run"].concurrency.clone();
        field.update(cx, |c, _| c.set_text("2"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        field.update(cx, |c, _| c.set_text("3"));
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"amendWorkflowRunSettings","runId":"run","toolCallId":"settings"}})), None, cx);
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        s.conversations.get_mut("parent").unwrap().workflow_runs.runs[0].concurrency = Some(crate::conversation::workflows_types::WorkflowRunConcurrency { ceiling: 8, limit: Some(2), ..Default::default() });
        s.reconcile_workflow_settings(&key, "parent", false);
        assert_eq!(field.read(cx).text(), "3");
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["params"]["payload"], json!({"workId":"run","maxConcurrency":3}));
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"failed","message":"token=secret"})), None, cx);
        s.conversations.get_mut("parent").unwrap().workflow_runs.runs[0].subagent_model = Some("other/model".into());
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        s.reload_workflow_settings(&key, "parent", "run", cx);
        assert_eq!(s.workspaces[0].workflow_settings["parent\0run"].model.read(cx).text(), "other/model");
        assert!(!s.workflow_settings_can_apply(&key, "parent", "run", cx));
    });
}

#[gpui::test]
fn interrupted_settings_retain_draft_and_require_fresh_projection(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        let mut conversation = ConversationState::default();
        conversation.workflow_runs.runs.push(WorkflowRunState {
            run_id: "run".into(),
            status: "running".into(),
            ..Default::default()
        });
        s.conversations.insert("parent".into(), conversation);
        s.open_workflow_settings(&key, "parent", "run", cx);
        let field = s.workspaces[0].workflow_settings["parent\0run"]
            .concurrency
            .clone();
        field.update(cx, |c, _| c.set_text("2"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_ok());
        s.workspaces[0].invalidate_connection();
        assert!(!s.workflow_settings_pending(&key, "parent", "run"));
        assert_eq!(field.read(cx).text(), "2");
        assert!(
            s.workspaces[0].workflow_settings["parent\0run"]
                .error
                .as_deref()
                .unwrap()
                .contains("unknown")
        );
        s.reload_workflow_settings(&key, "parent", "run", cx);
        assert_eq!(field.read(cx).text(), "2");
        s.reconcile_workflow_settings(&key, "parent", true);
        s.reload_workflow_settings(&key, "parent", "run", cx);
        assert_eq!(
            s.workspaces[0].workflow_settings["parent\0run"]
                .concurrency
                .read(cx)
                .text(),
            ""
        );
        s.conversations
            .get_mut("parent")
            .unwrap()
            .workflow_runs
            .runs[0]
            .status = "completed".into();
        s.reconcile_workflow_settings(&key, "parent", false);
        assert!(
            !s.workspaces[0]
                .workflow_settings
                .contains_key("parent\0run")
        );
    });
}
