use crate::app::store::AppState;
use crate::conversation::{
    model::ConversationState,
    workflows_types::{WorkflowRunState, WorkflowRunsState},
};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn workflow_settings_catalog_ceiling_and_uncertain_outcomes(cx: &mut TestAppContext) {
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
            concurrency_ceiling: Some(8),
            concurrency: Some(
                crate::conversation::workflows_types::WorkflowRunConcurrency {
                    ceiling: 8,
                    limit: Some(2),
                    ..Default::default()
                },
            ),
            ..Default::default()
        });
        s.conversations.insert("parent".into(), conversation);
        s.workspace_configs
            .entry(key.clone())
            .or_default()
            .models
            .push(
                crate::composer::catalog::ModelOption::from_value(
                    &json!({"value":"provider/model","modelThoughtLevels":["high"]}),
                )
                .unwrap(),
            );
        s.open_workflow_settings(&key, "parent", "run", cx);
        let form = &s.workspaces[0].workflow_settings["parent\0run"];
        let model = form.model.clone();
        let limit = form.concurrency.clone();
        model.update(cx, |c, _| c.set_text("provider/model$invalid"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        model.update(cx, |c, _| c.set_text("provider/model$high"));
        limit.update(cx, |c, _| c.set_text("8"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(
            request["params"]["payload"],
            json!({"workId":"run","subagentModel":"provider/model$high","maxConcurrency":null})
        );
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(json!({"status":"accepted","result":{}})),
            None,
            cx,
        );
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        assert!(
            s.workspaces[0].workflow_settings["parent\0run"]
                .awaiting
                .is_some()
        );
        s.reload_workflow_settings(&key, "parent", "run", cx);
        let limit = s.workspaces[0].workflow_settings["parent\0run"]
            .concurrency
            .clone();
        limit.update(cx, |c, _| c.set_text("1"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"token=secret"})),
            cx,
        );
        let form = &s.workspaces[0].workflow_settings["parent\0run"];
        assert!(form.awaiting.is_none());
        assert!(!form.error.as_ref().unwrap().contains("secret"));
        assert_eq!(limit.read(cx).text(), "1");
    });
}

#[gpui::test]
fn workflow_settings_omit_unchanged_reset_null_and_reject_stale_owner(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.conversations.insert("parent".into(), ConversationState {
            workflow_runs: WorkflowRunsState { runs: vec![WorkflowRunState { run_id: "run".into(), status: "running".into(), concurrency_ceiling: Some(8), subagent_model: Some("provider/model".into()), ..Default::default() }], ..Default::default() },
            ..Default::default()
        });
        s.open_workflow_settings(&key, "parent", "run", cx);
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        let model = s.workspaces[0].workflow_settings["parent\0run"].model.clone();
        model.update(cx, |c, _| c.set_text(""));
        s.apply_workflow_settings(&key, "other", "run", cx);
        assert!(rx.try_recv().is_err());
        s.apply_workflow_settings(&key, "parent", "run", cx);
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["params"]["type"], "amendWorkflowRunSettings");
        assert_eq!(request["params"]["payload"], json!({"workId":"run","subagentModel":null}));
        assert!(request["params"].get("baseRevision").is_none());
        assert!(rx.try_recv().is_err());
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"rejected","message":"token=sentinel-secret"})), None, cx);
        assert!(!s.workspaces[0].workflow_settings["parent\0run"].error.as_ref().unwrap().contains("sentinel-secret"));
        let concurrency = s.workspaces[0].workflow_settings["parent\0run"].concurrency.clone();
        concurrency.update(cx, |c, _| c.set_text("0"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
        concurrency.update(cx, |c, _| c.set_text("2"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["params"]["payload"]["maxConcurrency"], 2);
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"amendWorkflowRunSettings","runId":"new","toolCallId":"settings"}})), None, cx);
        assert_eq!(s.conversations["parent"].workflow_runs.runs[0].run_id, "run");
        assert_eq!(concurrency.read(cx).text(), "2");
        s.conversations.get_mut("parent").unwrap().workflow_runs.runs[0].status = "completed".into();
        s.apply_workflow_settings(&key, "parent", "run", cx);
        assert!(rx.try_recv().is_err());
    });
}
