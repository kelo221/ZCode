use crate::app::{
    dock::DockTab,
    plugin_pane::PluginSegment,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

fn workspace_fixture() -> Value {
    let mut fixture = crate::shared::plugin_config::tests::fixture();
    // Clear 只能清除当前范围拥有的覆盖值；工作区表单不能用 user 来源验证该按钮。
    fixture["plugins"][0]["optionSources"]["count"] = json!("workspace");
    fixture
}

#[test]
fn plugin_config_masked_input_save_reload_and_reset_are_pointer_reachable() {
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
    let key = app.update_entity(&state,|s,cx| {
        let ws = &mut s.workspaces[0]; ws.inbound = Some(tx); ws.started = true;
        ws.inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        ws.inspection.plugins.attempted = true; let key = ws.key.clone(); s.active_workspace = Some(key.clone()); s.composer.update(cx,|c,_|c.set_text("parent kept")); key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(1000.)));
    window.update(|v, window, cx| {
        window.bounds_changed(cx);
        v.dock_open = true;
        v.dock_tab = DockTab::Plugins;
        v.plugin_segment = PluginSegment::ManageInstalled;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-fixture@local"]);
    window.simulate_click(target.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(get["method"], "plugins/list");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(workspace_fixture()),
            None,
            cx,
        )
    });
    window.draw();
    let scroll_origin =
        app.update(|cx| cx.global::<TestTargets>().0["plugin-config-reload"].center());
    window.simulate_scroll(scroll_origin, gpui::point(px(0.), px(-320.)));
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-option-token"]);
    window.simulate_click(target.center(), MouseButton::Left);
    window.simulate_input("fixture-secret");
    window.draw();
    app.read_entity(&state,|s,_| assert!(matches!(&s.workspaces[0].inspection.plugin_config.as_ref().unwrap().edits["token"].0,crate::shared::plugin_config::ConfigEdit::Text(v) if v == "fixture-secret")));
    window.simulate_scroll(target.center(), gpui::point(px(0.), px(600.)));
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-save"]);
    window.simulate_click(target.center(), MouseButton::Left);
    let preflight: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(preflight["method"], "plugins/list");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            preflight["id"].as_u64().unwrap(),
            Some(workspace_fixture()),
            None,
            cx,
        )
    });
    let write: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(write["params"]["options"]["token"], "fixture-secret");
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            write["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"token=secret"})),
            cx,
        )
    });
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-reload"]);
    window.simulate_click(target.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(workspace_fixture()),
            None,
            cx,
        )
    });
    window.draw();
    let scroll_origin =
        app.update(|cx| cx.global::<TestTargets>().0["plugin-config-reload"].center());
    window.simulate_scroll(scroll_origin, gpui::point(px(0.), px(-400.)));
    window.draw();
    app.read(|cx| {
        assert!(
            !cx.global::<TestTargets>()
                .0
                .contains_key("plugin-option-clear-token")
        );
    });
    let old_input = app.read_entity(&state, |s, _| {
        s.workspaces[0]
            .inspection
            .plugin_config
            .as_ref()
            .unwrap()
            .inputs["count"]
            .clone()
    });
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-option-clear-count"]);
    window.simulate_click(target.center(), MouseButton::Left);
    window.draw();
    app.read_entity(&state, |s, _| {
        assert!(matches!(
            &s.workspaces[0]
                .inspection
                .plugin_config
                .as_ref()
                .unwrap()
                .edits["count"]
                .0,
            crate::shared::plugin_config::ConfigEdit::Clear
        ))
    });
    app.update_entity(&old_input, |input, cx| input.set_text("99", cx));
    app.read_entity(&state, |s, cx| {
        let form = s.workspaces[0].inspection.plugin_config.as_ref().unwrap();
        assert!(matches!(
            form.edits["count"].0,
            crate::shared::plugin_config::ConfigEdit::Clear
        ));
        assert_ne!(form.inputs["count"], old_input);
        assert_eq!(form.inputs["count"].read(cx).text(), "");
    });
    window.simulate_scroll(target.center(), gpui::point(px(0.), px(600.)));
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-reset"]);
    window.simulate_click(target.center(), MouseButton::Left);
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-cancel"]);
    window.simulate_click(target.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-reset"]);
    let scroll_origin = target.center();
    window.simulate_click(scroll_origin, MouseButton::Left);
    window.draw();
    app.read_entity(&state, |s, _| {
        assert!(
            s.workspaces[0]
                .inspection
                .plugin_config
                .as_ref()
                .unwrap()
                .confirmation
        )
    });
    let target = app.update(|cx| cx.global::<TestTargets>().0["plugin-config-confirm"]);
    window.simulate_click(target.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            get["id"].as_u64().unwrap(),
            Some(workspace_fixture()),
            None,
            cx,
        )
    });
    let reset: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(reset["method"], "plugins/resetConfig");
    assert_eq!(reset["params"]["scope"], "workspace");
    app.read_entity(&state, |s, cx| {
        assert_eq!(s.composer.read(cx).text(), "parent kept")
    });
}
