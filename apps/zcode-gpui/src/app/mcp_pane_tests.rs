use crate::app::root::RootView;
use crate::app::settings::SettingsSection;
use crate::app::store::AppState;
use crate::app::test_support::{RUNTIME_TEST_LOCK, TestTargets};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn mcp_settings_renders_groups_and_refreshes_in_connect_mode() {
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
    let key = app.update_entity(&state, |s, _| {
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key.clone());
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
    assert_eq!(request["params"]["mode"], "connect");

    let row = |status: &str| {
        json!({"status":status,"transport":"stdio","toolCount":4,
            "updatedAt":"2026-10-22T10:00:00Z"})
    };
    let mut failed = row("failed");
    failed["failureKind"] = json!("process_start_failed");
    failed["error"] = json!("raw diagnostic");
    let mut plugin = row("connecting");
    plugin["transport"] = json!("http");
    plugin["authorization"] = json!({"type":"oauth_authorization_code",
        "authorizationUrl":"https://auth.example.test/authorize","startedAt":"2026-10-22T10:00:00Z"});
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(json!({"statuses":{
                "blender-mcp": row("connected"),
                "broken": failed,
                "plugin:figma:figma": plugin,
            }})),
            None,
            cx,
        );
    });
    window.draw();
    app.read(|cx| {
        let targets = &cx.global::<TestTargets>().0;
        assert!(targets.contains_key("mcp-refresh"));
        // Plugin rows keep the full runtime name for authorization receipts.
        assert!(targets.contains_key("mcp-authorize-plugin:figma:figma"));
    });
    let refresh = app.read(|cx| cx.global::<TestTargets>().0["mcp-refresh"].center());
    window.simulate_click(refresh, MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "mcp/list");
    assert_eq!(request["params"]["mode"], "connect");
    assert!(request["params"].get("mcpServers").is_none());
    // The previous list stays visible while the refresh loads.
    app.read_entity(&state, |s, _| {
        let mcp = &s.active_inspection().unwrap().mcp;
        assert!(mcp.loading);
        assert_eq!(mcp.value.as_ref().unwrap().len(), 3);
    });
}
