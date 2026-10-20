use crate::app::root::RootView;
use crate::app::settings::SettingsSection;
use crate::app::store::AppState;
use crate::app::test_support::{RUNTIME_TEST_LOCK, TestTargets};
use crate::backend::launcher::ConnEvent;
use crate::shared::preferences::{PreferenceOwner, Preferences};
use crate::shared::settings::{AppSettings, load_settings_from_path};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::{Arc, mpsc};

fn request(id: u64) -> ConnEvent {
    ConnEvent::Line(
        json!({"id":id,"method":"session/requestRuntimePreferences",
        "params":{"sessionId":"fixture-session","scope":"runtime-materialization"}})
        .to_string(),
    )
}

#[test]
fn lsp_diagnostics_defaults_on_and_the_switch_turns_it_off_for_new_sessions() {
    assert!(AppSettings::default().lsp_diagnostics());
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-lsp-diagnostics-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("setting.json");
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            load_settings_from_path(&path),
            path.clone(),
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = mpsc::channel();
    let key = app.update_entity(&state, |s, _| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].key.clone()
    });
    app.update_entity(&state, |s, cx| {
        assert!(s.handle_conn(&key, request(1), cx));
    });
    let on: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(on["result"]["lspDiagnosticsEnabled"], true);

    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    let gear = app.read(|cx| cx.global::<TestTargets>().0["open-settings"].center());
    window.simulate_click(gear, MouseButton::Left);
    window.draw();
    let section = app.read(|cx| cx.global::<TestTargets>().0["settings-diagnostics"].center());
    window.simulate_click(section, MouseButton::Left);
    window.read(|v, _| assert!(v.settings.section == SettingsSection::Diagnostics));
    window.draw();
    let switch =
        app.read(|cx| cx.global::<TestTargets>().0["settings-lsp-diagnostics-enabled"].center());
    window.simulate_click(switch, MouseButton::Left);
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert_eq!(owner.snapshot.gpui_lsp_diagnostics, Some(false));
    });

    app.update_entity(&state, |s, cx| {
        assert!(s.handle_conn(&key, request(2), cx));
    });
    let off: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(off["result"]["lspDiagnosticsEnabled"], false);
    assert_eq!(
        load_settings_from_path(&path).gpui_lsp_diagnostics,
        Some(false)
    );
    drop(window);
    std::fs::remove_dir_all(dir).unwrap();
}
