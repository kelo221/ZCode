use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::{model::ConversationState, workflows_types::WorkflowRunState};
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

#[test]
fn workflow_successor_and_all_runs_are_pointer_reachable() {
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
    app.update_entity(&state, |s, cx| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        ws.saved_workflows
            .entry("project".into())
            .or_default()
            .attempted = true;
        s.active_workspace = Some(ws.key.clone());
        s.active = Some("parent".into());
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        let mut conv = ConversationState::default();
        conv.workflow_runs.runs = vec![
            WorkflowRunState {
                run_id: "source".into(),
                tool_call_id: Some("source-tool".into()),
                status: "stopped".into(),
                superseded_by: Some("target".into()),
                ..Default::default()
            },
            WorkflowRunState {
                run_id: "target".into(),
                tool_call_id: Some("target-tool".into()),
                status: "completed".into(),
                ..Default::default()
            },
        ];
        s.conversations.insert("parent".into(), conv);
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(1100.)));
    window.update(|view, window, cx| {
        window.bounds_changed(cx);
        view.dock_open = true;
        view.dock_tab = DockTab::Workflows;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let successor =
        app.read(|cx| cx.global::<TestTargets>().0["workflow-successor-source"].center());
    window.simulate_click(successor, MouseButton::Left);
    app.read_entity(&state, |s, cx| {
        assert_eq!(s.workflow_focus().unwrap().run, "target");
        assert_eq!(s.composer.read(cx).text(), "parent draft");
    });
    window.draw();
    let all = app.read(|cx| cx.global::<TestTargets>().0["workflow-all-runs"].center());
    window.simulate_click(all, MouseButton::Left);
    app.read_entity(&state, |s, _| assert!(s.workflow_focus().is_none()));
    window.draw();
    let open = app.read(|cx| cx.global::<TestTargets>().0["workflow-open-source"].center());
    window.simulate_click(open, MouseButton::Left);
    app.read_entity(&state, |s, _| {
        assert_eq!(s.workflow_focus().unwrap().run, "source")
    });
    let reads = rx
        .try_iter()
        .map(|line| serde_json::from_str::<serde_json::Value>(&line).unwrap())
        .collect::<Vec<_>>();
    assert!(!reads.is_empty());
    assert!(reads.iter().all(
        |read| read["method"] == "v4/conversation/workflowRunArtifacts"
            && read["params"]["sessionId"] == "parent"
            && matches!(read["params"]["runId"].as_str(), Some("source" | "target"))
    ));
}
