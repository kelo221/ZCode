use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::{model::ConversationState, workflows_types::WorkflowRunState};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn workflow_settings_configure_apply_and_reload_are_pointer_reachable() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    let key = app.update_entity(&state, |s, cx| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        ws.saved_workflows
            .entry("project".into())
            .or_default()
            .attempted = true;
        let key = ws.key.clone();
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        let mut conversation = ConversationState::default();
        conversation.workflow_runs.runs.push(WorkflowRunState {
            run_id: "run".into(),
            tool_call_id: Some("run-tool".into()),
            status: "running".into(),
            concurrency_ceiling: Some(8),
            ..Default::default()
        });
        s.conversations.insert("parent".into(), conversation);
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(1100.)));
    window.update(|view, _, cx| {
        view.dock_open = true;
        view.dock_tab = DockTab::Workflows;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let configure = app.update(|cx| cx.global::<TestTargets>().0["configure-wf-run"]);
    window.simulate_click(configure.center(), MouseButton::Left);
    window.draw();
    let apply = app.update(|cx| cx.global::<TestTargets>().0["apply-wf-run"]);
    window.simulate_click(apply.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    let limit = app.update(|cx| cx.global::<TestTargets>().0["wf-limit-run"]);
    window.simulate_click(limit.center(), MouseButton::Left);
    window.simulate_input("2");
    window.draw();
    let apply = app.update(|cx| cx.global::<TestTargets>().0["apply-wf-run"]);
    window.simulate_click(apply.center(), MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(
        request["params"]["payload"],
        json!({"workId":"run","maxConcurrency":2})
    );
    window.draw();
    window.simulate_click(apply.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(json!({"status":"rejected","message":"Rejected settings"})),
            None,
            cx,
        )
    });
    window.draw();
    let reload = app.update(|cx| cx.global::<TestTargets>().0["reload-wf-run"]);
    window.simulate_click(reload.center(), MouseButton::Left);
    window.draw();
    app.read_entity(&state, |s, cx| {
        assert_eq!(s.composer.read(cx).text(), "parent draft");
        assert_eq!(
            s.workspaces[0].workflow_settings["parent\0run"]
                .concurrency
                .read(cx)
                .text(),
            ""
        );
        assert!(!s.workflow_settings_can_apply(&key, "parent", "run", cx));
    });
    app.update_entity(&state, |s, cx| {
        assert!(s.focus_workflow(&key, "parent", "run", "run-tool"));
        let input = s.workspaces[0].workflow_settings["parent\0run"]
            .concurrency
            .clone();
        input.update(cx, |c, _| c.set_text("2"));
        s.apply_workflow_settings(&key, "parent", "run", cx);
    });
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"accepted","result":{"type":"amendWorkflowRunSettings","runId":"successor","toolCallId":"successor-tool"}})), None, cx);
        assert_eq!(s.workflow_focus().unwrap().run, "run");
        let runs = &mut s.conversations.get_mut("parent").unwrap().workflow_runs.runs;
        runs[0].status = "stopped".into();
        runs[0].superseded_by = Some("successor".into());
        runs.push(WorkflowRunState { run_id: "successor".into(), tool_call_id: Some("successor-tool".into()), status: "running".into(), ..Default::default() });
        s.reconcile_workflow_settings(&key, "parent", false);
        assert_eq!(s.workflow_focus().unwrap().run, "successor");
        assert_eq!(s.composer.read(cx).text(), "parent draft");
    });
    let reads = rx
        .try_iter()
        .map(|line| serde_json::from_str::<Value>(&line).unwrap())
        .collect::<Vec<_>>();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(
        |read| read["method"] == "v4/conversation/workflowRunArtifacts"
            && read["params"]["sessionId"] == "parent"
            && matches!(read["params"]["runId"].as_str(), Some("run" | "successor"))
    ));
}
