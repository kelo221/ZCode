use crate::app::root::RootView;
use crate::app::settings::SettingsSection;
use crate::app::store::AppState;
use crate::app::test_support::{RUNTIME_TEST_LOCK, TestTargets};
use crate::shared::mcp::authorization_tests::fixture;
use gpui::{MouseButton, TestApp, px, size};
use serde_json::Value;
use std::sync::Arc;

#[test]
fn mcp_authorization_is_pointer_reachable_and_refresh_removes_action() {
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
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key.clone());
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|v, window, cx| {
        window.bounds_changed(cx);
        v.open_settings(window, cx);
        v.select_settings_section(SettingsSection::Mcp, cx);
    });
    window.draw();
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "mcp/list");
    assert_eq!(request["params"]["mode"], "status");
    assert!(request["params"].get("mcpServers").is_none());
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(fixture(Some(
                "https://auth.example.test/authorize?state=fixture-token",
            ))),
            None,
            cx,
        );
    });
    window.draw();
    assert!(app.opened_url().is_none());
    let target = app.read(|cx| cx.global::<TestTargets>().0["mcp-authorize-fixture"].center());
    window.simulate_click(target, MouseButton::Left);
    assert_eq!(
        app.opened_url().as_deref(),
        Some("https://auth.example.test/authorize?state=fixture-token")
    );
    let receipt = app.read_entity(&state, |s, _| {
        s.mcp_authorization_receipt("fixture").unwrap()
    });
    window.draw();
    let refresh = app.read(|cx| cx.global::<TestTargets>().0["mcp-refresh"].center());
    window.simulate_click(refresh, MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(fixture(None)),
            None,
            cx,
        );
    });
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    app.read(|cx| {
        assert!(
            !cx.global::<TestTargets>()
                .0
                .contains_key("mcp-authorize-fixture")
        )
    });
    app.read_entity(&state, |s, _| {
        let mut calls = 0;
        assert!(!s.open_mcp_authorization(&receipt, |_| calls += 1));
        assert_eq!(calls, 0);
    });
    window.simulate_keystroke("escape");
    window.read(|v, cx| {
        assert!(!v.settings.open);
        assert_eq!(v.state.read(cx).composer.read(cx).text(), "parent draft");
    });
}
