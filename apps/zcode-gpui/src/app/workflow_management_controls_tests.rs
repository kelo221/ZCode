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
fn workflow_management_save_cancel_and_delete_are_pointer_reachable() {
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
    let value = json!({"ok":true,"name":"test","scope":"project","path":"/test","meta":{"description":"Test"},"script":""});
    let key = app.update_entity(&state, |s, cx| {
        let ws = &mut s.workspaces[0]; ws.inbound = Some(tx); ws.started = true;
        ws.saved_workflows.entry("project".into()).or_default().finish(SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","scope":"project","path":"/test","description":"Test"}],"invalid":[],"dir":"/saved"}), "project").unwrap());
        ws.saved_workflows.get_mut("project").unwrap().attempted = true;
        ws.saved_workflow_form.selected = Some("test".into());
        ws.workflow_definitions.entry("project\0test".into()).or_default().finish(WorkflowDefinition::parse(&value, "project", "test").unwrap());
        let key = ws.key.clone(); s.active_workspace = Some(key.clone()); s.active = Some("parent".into());
        s.composer.update(cx, |c, _| c.set_text("parent draft")); key
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
    let save = app.update(|cx| cx.global::<TestTargets>().0["workflow-meta-save"]);
    window.simulate_click(save.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    let input = app.update(|cx| cx.global::<TestTargets>().0["workflow-meta-description"]);
    window.simulate_click(input.center(), MouseButton::Left);
    window.simulate_input(" changed");
    app.read_entity(&state, |s, cx| {
        let form = s.workspaces[0].workflow_management.as_ref().unwrap();
        assert_eq!(form.description.read(cx).text(), "Test changed");
        assert!(form.dirty(cx));
    });
    window.draw();
    window.simulate_scroll(point(manage.center().x, px(420.)), point(px(0.), px(-320.)));
    window.draw();
    let save = app.update(|cx| cx.global::<TestTargets>().0["workflow-meta-save"]);
    assert!(save.bottom() < px(620.), "Save bounds: {save:?}");
    window.simulate_click(save.center(), MouseButton::Left);
    app.read_entity(&state, |s, cx| {
        assert!(
            s.workflow_management_pending(&key),
            "save={save:?}, dirty={}, read_only={}, scope={}, selected={:?}, needs_reload={}",
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .dirty(cx),
            s.is_read_only_view(),
            s.workspaces[0].saved_workflow_form.scope,
            s.workspaces[0].saved_workflow_form.selected,
            s.workspaces[0]
                .workflow_management
                .as_ref()
                .unwrap()
                .needs_reload
        );
    });
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(get["method"], "workflows/get");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(value.clone()),
            None,
            cx,
        )
    });
    let write: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(write["method"], "workflows/updateMeta");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":false,"reason":"read_error"})),
            None,
            cx,
        )
    });
    window.draw();
    let reload = app.update(|cx| cx.global::<TestTargets>().0["workflow-meta-reload"]);
    window.simulate_click(reload.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(value.clone()),
            None,
            cx,
        )
    });
    window.draw();
    let delete = app.update(|cx| cx.global::<TestTargets>().0["workflow-delete"]);
    window.simulate_click(delete.center(), MouseButton::Left);
    window.draw();
    assert!(rx.try_recv().is_err());
    let cancel = app.update(|cx| cx.global::<TestTargets>().0["workflow-cancel"]);
    window.simulate_click(cancel.center(), MouseButton::Left);
    window.draw();
    assert!(rx.try_recv().is_err());
    window.simulate_click(delete.center(), MouseButton::Left);
    window.draw();
    let confirm = app.update(|cx| cx.global::<TestTargets>().0["workflow-confirm"]);
    window.simulate_click(confirm.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(&key, get["id"].as_u64().unwrap(), Some(value), None, cx)
    });
    let write: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(write["method"], "workflows/delete");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            Some(json!({"ok":true,"path":"/test"})),
            None,
            cx,
        )
    });
    app.read_entity(&state, |s, cx| {
        assert_eq!(s.active.as_deref(), Some("parent"));
        assert_eq!(s.composer.read(cx).text(), "parent draft");
        assert!(s.workspaces[0].saved_workflow_form.selected.is_none());
    });
}
