use crate::app::{
    dock::DockTab,
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn plugin_details_refresh_and_back_are_pointer_reachable() {
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
        let ws = &mut s.workspaces[0]; ws.inbound = Some(tx); ws.started = true;
        ws.inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[{"id":"zcode-plugins-official","name":"Official","source":{},"pluginCount":1,"isOfficial":true}],"availablePlugins":[{"id":"p","name":"plugin","marketplace":"zcode-plugins-official","installed":false}],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        ws.inspection.plugins.attempted = true; let key = ws.key.clone(); s.active_workspace = Some(key.clone()); s.composer.update(cx, |c, _| c.set_text("kept")); key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(1000.)));
    window.update(|v, _, cx| {
        v.dock_open = true;
        v.dock_tab = DockTab::Plugins;
        cx.notify();
    });
    window.draw();
    while rx.try_recv().is_ok() {}
    let details = app.update(|cx| cx.global::<TestTargets>().0["plugin-details-p"]);
    window.simulate_click(details.center(), MouseButton::Left);
    let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(get["method"], "plugins/describe");
    window.draw();
    let refresh = app.update(|cx| cx.global::<TestTargets>().0["plugin-detail-refresh"]);
    window.simulate_click(refresh.center(), MouseButton::Left);
    assert!(rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| s.handle_response(&key, get["id"].as_u64().unwrap(), Some(json!({"components":[{"kind":"skill","items":[{"name":"Fixture","description":"Test"}]}]})), None, cx));
    window.draw();
    let refresh = app.update(|cx| cx.global::<TestTargets>().0["plugin-detail-refresh"]);
    window.simulate_click(refresh.center(), MouseButton::Left);
    let retry: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    app.update_entity(&state, |s, cx| {
        s.handle_response(
            &key,
            retry["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"Read failed"})),
            cx,
        )
    });
    window.draw();
    let close = app.update(|cx| cx.global::<TestTargets>().0["plugin-detail-close"]);
    window.simulate_click(close.center(), MouseButton::Left);
    window.draw();
    app.read_entity(&state, |s, cx| {
        assert!(s.workspaces[0].inspection.plugin_detail.is_none());
        assert_eq!(s.composer.read(cx).text(), "kept");
    });
}
