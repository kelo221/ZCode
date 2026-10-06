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

#[test]
fn personal_source_add_refresh_and_management_buttons_are_pointer_reachable() {
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
        ws.inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"tool","name":"Tool","marketplace":"personal","enabled":true,"scope":"user"}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        let key = ws.key.clone();
        s.active_workspace = Some(key.clone());
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|view, _, cx| {
        view.dock_open = true;
        view.dock_tab = DockTab::Plugins;
        view.plugin_segment = PluginSegment::Personal;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let field = app.update(|cx| cx.global::<TestTargets>().0["plugin-source-input"]);
    window.simulate_click(field.center(), MouseButton::Left);
    window.simulate_input("https://example.invalid/plugins");
    window.draw();
    let add = app.update(|cx| cx.global::<TestTargets>().0["add-plugin-source"]);
    window.simulate_click(add.center(), MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "plugins/marketplace/add");
    assert_eq!(
        request["params"]["source"],
        "https://example.invalid/plugins"
    );
    window.draw();
    window.simulate_click(add.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"diagnostics":[{"code":"failed","severity":"error","message":"token=sentinel-secret"}]})), None, cx));
    window.draw();
    let refresh = app.update(|cx| cx.global::<TestTargets>().0["refresh-plugins"]);
    window.simulate_click(refresh.center(), MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "plugins/marketplace/update");
    assert!(request["params"].get("marketplace").is_none());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(json!({"diagnostics":[]})),
            None,
            cx,
        )
    });
    while rx.try_recv().is_ok() {}
    window.update(|view, window, cx| {
        view.open_settings(window, cx);
        view.select_settings_section(crate::app::settings::SettingsSection::Plugins, cx);
    });
    window.draw();
    let toggle = app.update(|cx| cx.global::<TestTargets>().0["toggle-tool"]);
    window.simulate_click(toggle.center(), MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "plugins/setEnabled");
    assert_eq!(request["params"]["enabled"], false);
    window.read(|view, cx| {
        let state = view.state.read(cx);
        assert_eq!(state.composer.read(cx).text(), "parent draft");
        assert_eq!(
            state.workspaces[0]
                .plugin_source_draft
                .input
                .as_ref()
                .unwrap()
                .read(cx)
                .text(),
            "https://example.invalid/plugins"
        );
    });
    app.update_entity(&state, |s, _| {
        s.workspaces[0].pending.clear();
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[{"id":"personal","name":"Personal","pluginCount":1}],"availablePlugins":[{"id":"tool","name":"tool","marketplace":"personal","installed":true,"listing":{"examplePrompts":["Example"]}}],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        s.active = Some("parent".into()); s.draft = false;
    });
    window.update(|view, window, cx| {
        view.close_settings(window, cx);
        view.plugin_segment = PluginSegment::Personal;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let prompt = app.update(|cx| cx.global::<TestTargets>().0["plugin-prompt-tool-0"]);
    window.simulate_click(prompt.center(), MouseButton::Left);
    let reference: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(reference["method"], "plugins/referenceCatalog");
    window.draw();
    window.simulate_click(prompt.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| s.handle_response(&key, reference["id"].as_u64().unwrap(), Some(json!({"authority":"workspace","plugins":[{"pluginId":"tool","name":"tool","marketplace":"personal","enabled":true,"conflictingPluginIds":[],"skillQualifiedNames":[],"mcpServerNames":[]}]})), None, cx));
    app.read_entity(&state, |s, cx| {
        assert!(s.active.is_none());
        assert_eq!(s.composer.read(cx).text(), "[@tool](plugin://tool) Example");
        assert_eq!(s.session_drafts["parent"], "parent draft");
    });
    assert!(rx.try_recv().is_err());
}
