use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::shared::saved_workflows::SavedWorkflowList;
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn saved_workflow_select_argument_and_launch_are_pointer_reachable() {
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
        let key = ws.key.clone();
        ws.saved_workflows.entry("project".into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","description":"Test","scope":"project","path":"/test.ts","args":{"count":{"type":"number","required":true}}}],"invalid":[],"dir":"/workflows"}), "project").unwrap());
        ws.saved_workflows.get_mut("project").unwrap().attempted = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.draft = false;
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|view, _, cx| {
        view.dock_open = true;
        view.dock_tab = DockTab::Workflows;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let global = app.update(|cx| cx.global::<TestTargets>().0["saved-scope-global"]);
    window.simulate_click(global.center(), MouseButton::Left);
    let list: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(list["params"]["scope"], "global");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            list["id"].as_u64().unwrap(),
            Some(json!({"workflows":[],"invalid":[],"dir":"/global"})),
            None,
            cx,
        )
    });
    window.draw();
    let project = app.update(|cx| cx.global::<TestTargets>().0["saved-scope-project"]);
    window.simulate_click(project.center(), MouseButton::Left);
    window.draw();
    app.read_entity(&state, |s, _| {
        assert_eq!(s.workspaces[0].saved_workflow_form.scope, "project")
    });
    let refresh = app.update(|cx| cx.global::<TestTargets>().0["saved-refresh"]);
    assert!(refresh.size.width > px(0.) && refresh.size.height > px(0.));
    window.simulate_click(refresh.center(), MouseButton::Left);
    let list: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(list["params"]["scope"], "project");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            list["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"Refresh failed"})),
            cx,
        )
    });
    window.draw();
    let select = app.update(|cx| cx.global::<TestTargets>().0["saved-select-test"]);
    window.simulate_click(select.center(), MouseButton::Left);
    window.draw();
    let definition = app.update(|cx| cx.global::<TestTargets>().0["saved-definition"]);
    window.simulate_click(definition.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(get["method"], "workflows/get");
    window.draw();
    window.simulate_click(definition.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| s.handle_response(&key, get["id"].as_u64().unwrap(), Some(json!({"ok":true,"name":"test","scope":"project","path":"/test.ts","meta":{"description":"Test"},"script":"return 1;"})), None, cx));
    window.draw();
    let history = app.update(|cx| cx.global::<TestTargets>().0["saved-history"]);
    window.simulate_click(history.center(), MouseButton::Left);
    let runs: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(runs["method"], "workflows/runs");
    window.draw();
    window.simulate_click(history.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            runs["id"].as_u64().unwrap(),
            Some(json!({"runs":[]})),
            None,
            cx,
        )
    });
    window.draw();
    let argument = app.update(|cx| cx.global::<TestTargets>().0["saved-arg-count"]);
    window.simulate_click(argument.center(), MouseButton::Left);
    window.simulate_input("4");
    window.draw();
    let launch = app.update(|cx| cx.global::<TestTargets>().0["saved-launch"]);
    window.simulate_click(launch.center(), MouseButton::Left);
    let create: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(create["params"]["type"], "createSession");
    window.draw();
    window.simulate_click(launch.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            create["id"].as_u64().unwrap(),
            Some(
                json!({"status":"accepted","result":{"type":"createSession","sessionId":"target"}}),
            ),
            None,
            cx,
        )
    });
    let start: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(start["params"]["payload"]["args"], json!({"count":4}));
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            start["id"].as_u64().unwrap(),
            Some(json!({"status":"failed","message":"Workflow capability unavailable"})),
            None,
            cx,
        )
    });
    window.read(|view, cx| {
        assert_eq!(view.state.read(cx).active.as_deref(), Some("parent"));
        assert_eq!(view.state.read(cx).composer.read(cx).text(), "parent draft");
    });
}
