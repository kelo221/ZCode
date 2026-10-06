use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::workflows_types::WorkflowRunState;
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn artifact_open_refresh_and_all_runs_are_pointer_reachable_without_draft_changes() {
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
        key
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
    let open = app.read(|cx| cx.global::<TestTargets>().0["workflow-open-run"].center());
    window.simulate_click(open, MouseButton::Left);
    window.draw();
    let req: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(req["method"], "v4/conversation/workflowRunArtifacts");
    assert_eq!(req["params"], json!({"sessionId":"parent","runId":"run"}));
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(json!({"artifacts":[{
                "id":"report","title":"Report","kind":"markdown","contentType":"text/markdown","version":1,
                "versions":[{"version":1,"publishedAt":1}],"itemCount":0,"primary":true,
            }]})),
            None,
            cx,
        );
    });
    window.draw();
    assert!(rx.try_recv().is_err());
    let view = app.read(|cx| cx.global::<TestTargets>().0["view-markdown-report"].center());
    window.simulate_click(view, MouseButton::Left);
    let read: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(read["method"], "v4/conversation/workflowRunArtifactRead");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            read["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"secret-sentinel"})),
            cx,
        )
    });
    window.draw();
    let retry = app.read(|cx| cx.global::<TestTargets>().0["retry-markdown"].center());
    window.simulate_click(retry, MouseButton::Left);
    let read: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| s.handle_response(&key, read["id"].as_u64().unwrap(), Some(json!({"dataBase64":"IyBSZXBvcnQ=","mediaType":"text/markdown","totalBytes":8,"nextOffset":null})), None, cx));
    window.draw();
    let close = app.read(|cx| cx.global::<TestTargets>().0["close-markdown"].center());
    window.simulate_click(close, MouseButton::Left);
    app.read_entity(&state, |s, _| {
        assert!(s.active_workflow_artifacts().unwrap().content.is_none())
    });
    window.draw();
    let refresh =
        app.read(|cx| cx.global::<TestTargets>().0["workflow-artifacts-refresh"].center());
    window.simulate_click(refresh, MouseButton::Left);
    let req: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    window.draw();
    window.simulate_click(refresh, MouseButton::Left);
    assert!(rx.try_recv().is_err());
    let all = app.read(|cx| cx.global::<TestTargets>().0["workflow-all-runs"].center());
    window.simulate_click(all, MouseButton::Left);
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            req["id"].as_u64().unwrap(),
            Some(json!({"artifacts":[]})),
            None,
            cx,
        );
        assert!(s.workflow_focus().is_none());
        assert!(s.active_workflow_artifacts().is_none());
        assert_eq!(s.composer.read(cx).text(), "parent draft");
    });
}
