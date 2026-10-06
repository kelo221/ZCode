use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::shared::{saved_workflows::SavedWorkflowList, workflow_definition::WorkflowDefinition};
use gpui::{MouseButton, TestApp, point, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn global_workflow_move_confirmation_is_pointer_reachable_and_scoped() {
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
    let value = json!({"ok":true,"name":"test","scope":"global","path":"/global/test","meta":{"description":"Test"},"script":""});
    let key = app.update_entity(&state, |s, _| {
        let ws = &mut s.workspaces[0]; ws.inbound = Some(tx); ws.started = true;
        ws.saved_workflows.entry("global".into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","scope":"global","path":"/global/test","description":"Test"}],"invalid":[],"dir":"/global"}), "global").unwrap());
        ws.saved_workflows.get_mut("global").unwrap().attempted = true;
        ws.saved_workflow_form.scope = "global".into(); ws.saved_workflow_form.selected = Some("test".into());
        ws.workflow_definitions.entry("global\0test".into()).or_default().finish(WorkflowDefinition::parse(&value, "global", "test").unwrap());
        let key = ws.key.clone(); s.active_workspace = Some(key.clone()); key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(1200.)));
    window.update(|view, _, cx| {
        view.dock_open = true;
        view.dock_tab = DockTab::Workflows;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let manage = app.update(|cx| cx.global::<TestTargets>().0["workflow-manage"]);
    window.simulate_click(manage.center(), MouseButton::Left);
    window.draw();
    window.simulate_scroll(point(manage.center().x, px(420.)), point(px(0.), px(-350.)));
    window.draw();
    let moving = app.update(|cx| cx.global::<TestTargets>().0["workflow-move"]);
    window.simulate_click(moving.center(), MouseButton::Left);
    window.draw();
    assert!(rx.try_recv().is_err());
    window.simulate_scroll(point(manage.center().x, px(420.)), point(px(0.), px(-350.)));
    window.draw();
    let confirm = app.update(|cx| cx.global::<TestTargets>().0["workflow-confirm"]);
    window.simulate_click(confirm.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(get["params"]["scope"], "global");
    app.update_entity(&state, |s, cx| {
        s.handle_response(&key, get["id"].as_u64().unwrap(), Some(value), None, cx)
    });
    let write: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(write["method"], "workflows/move");
    assert!(write["params"].get("scope").is_none());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":true,"from":"/global/test","to":"/project/test"})),
            None,
            cx,
        )
    });
    let a: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    let b: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(a["method"], "workflows/list");
    assert_eq!(b["method"], "workflows/list");
    assert_ne!(a["params"]["scope"], b["params"]["scope"]);
}
