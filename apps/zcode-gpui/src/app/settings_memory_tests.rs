use crate::app::root::RootView;
use crate::app::settings::SettingsSection;
use crate::app::store::AppState;
use crate::app::test_support::{RUNTIME_TEST_LOCK, TestTargets};
use crate::backend::launcher::ConnEvent;
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::settings::{AppSettings, load_settings_from_path};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, mpsc};

fn test_app(path: &Path, blocked: bool) -> TestApp {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            load_settings_from_path(path),
            path.into(),
            Arc::new(move || blocked),
            cx,
        );
    });
    app
}

fn request(id: u64, scope: &str) -> ConnEvent {
    ConnEvent::Line(
        json!({"id":id,"method":"session/requestRuntimePreferences",
        "params":{"sessionId":"fixture-session","scope":scope}})
        .to_string(),
    )
}

#[test]
fn memory_pointer_commit_and_rehydration_preserve_draft_and_focus() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-memory-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("setting.json");
    std::fs::write(&path, r#"{"unmodeled":{"kept":true}}"#).unwrap();
    {
        let mut app = test_app(&path, false);
        let state = app.new_entity(AppState::for_test);
        app.update_entity(&state, |s, cx| {
            s.composer.update(cx, |c, _| c.set_text("parent draft"))
        });
        let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
        window.simulate_resize(size(px(1280.), px(820.)));
        window.update(|_, window, cx| window.bounds_changed(cx));
        window.draw();
        let gear = app.read(|cx| cx.global::<TestTargets>().0["open-settings"].center());
        window.simulate_click(gear, MouseButton::Left);
        window.draw();
        let section = app.read(|cx| cx.global::<TestTargets>().0["settings-memory"].center());
        window.simulate_click(section, MouseButton::Left);
        window.read(|v, _| assert!(v.settings.section == SettingsSection::Memory));
        window.draw();
        let switch =
            app.read(|cx| cx.global::<TestTargets>().0["settings-memory-enabled"].center());
        window.simulate_click(switch, MouseButton::Left);
        app.read(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.read(cx);
            assert_eq!(owner.snapshot.memory_enabled, Some(true));
            assert!(!owner.saving);
            assert!(owner.error.is_none());
        });
        window.simulate_keystroke("escape");
        window.read(|v, cx| {
            assert!(!v.settings.open);
            assert_eq!(v.state.read(cx).composer.read(cx).text(), "parent draft");
        });
        window.update(|v, window, cx| {
            assert!(v.state.read(cx).composer.read(cx).focus.is_focused(window))
        });
    }
    let persisted = load_settings_from_path(&path);
    assert_eq!(persisted.memory_enabled, Some(true));
    assert_eq!(persisted.extra["unmodeled"]["kept"], true);
    let app = test_app(&path, false);
    app.read(|cx| {
        assert_eq!(
            cx.global::<PreferenceOwner>()
                .0
                .read(cx)
                .snapshot
                .memory_enabled,
            Some(true)
        )
    });
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_reverse_replies_read_only_committed_owner_and_origin_connection() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-memory-wire-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(&dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    let (a_tx, a_rx) = mpsc::channel();
    let (b_tx, b_rx) = mpsc::channel();
    let b = app.update_entity(&state, |s, _| {
        s.workspaces[0].inbound = Some(a_tx);
        let mut b = crate::backend::workspace::WorkspaceHandle::new("fixture-b".into(), vec![]);
        b.inbound = Some(b_tx);
        let key = b.key.clone();
        s.workspaces.push(b);
        s.active_workspace = Some(s.workspaces[0].key.clone());
        key
    });
    app.update(|cx| {
        Preferences::enqueue(PreferenceChange::MemoryEnabled(true), cx);
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert!(owner.saving);
        assert!(!owner.snapshot.memory_enabled.unwrap_or(false));
        state.update(cx, |s, cx| {
            assert!(s.handle_conn(&b, request(1, "runtime-materialization"), cx))
        });
    });
    let before: Value = serde_json::from_str(&b_rx.try_recv().unwrap()).unwrap();
    assert_eq!(before["result"]["memoryEnabled"], false);
    assert!(a_rx.try_recv().is_err());
    app.update_entity(&state, |s, cx| {
        assert!(s.handle_conn(&b, request(2, "runtime-materialization"), cx));
        assert!(s.handle_conn(&b, request(3, "user-execution"), cx));
    });
    for id in [2, 3] {
        let response: Value = serde_json::from_str(&b_rx.try_recv().unwrap()).unwrap();
        assert_eq!(response["id"], id);
        assert_eq!(response["result"]["memoryEnabled"], true);
        assert_eq!(response["result"]["nativeSearchEnhancementsEnabled"], true);
        assert_eq!(
            response["result"]["askUserQuestionAutoResolutionEnabled"],
            true
        );
    }
    assert!(a_rx.try_recv().is_err());
    app.update(|cx| Preferences::enqueue(PreferenceChange::MemoryEnabled(false), cx));
    app.update_entity(&state, |s, cx| {
        s.handle_conn(&b, request(4, "user-execution"), cx);
    });
    let disabled: Value = serde_json::from_str(&b_rx.try_recv().unwrap()).unwrap();
    assert_eq!(disabled["result"]["memoryEnabled"], false);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn memory_write_refusal_keeps_disk_snapshot_and_runtime_reply_disabled() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (blocked, contents) in [(true, r#"{"memoryEnabled":false}"#), (false, "{ invalid")] {
        let dir =
            std::env::temp_dir().join(format!("gpui-memory-refusal-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setting.json");
        std::fs::write(&path, contents).unwrap();
        let mut app = test_app(&path, blocked);
        let state = app.new_entity(AppState::for_test);
        let (tx, rx) = mpsc::channel();
        let key = app.update_entity(&state, |s, _| {
            s.workspaces[0].inbound = Some(tx);
            s.workspaces[0].key.clone()
        });
        app.update(|cx| Preferences::enqueue(PreferenceChange::MemoryEnabled(true), cx));
        app.read(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.read(cx);
            assert!(!owner.snapshot.memory_enabled.unwrap_or(false));
            assert!(owner.error.is_some());
            assert!(!owner.saving);
        });
        app.update_entity(&state, |s, cx| {
            s.handle_conn(&key, request(1, "runtime-materialization"), cx);
        });
        let response: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(response["result"]["memoryEnabled"], false);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn memory_typed_setting_defaults_and_mutations_preserve_unknown_fields() {
    assert_eq!(AppSettings::default().memory_enabled, None);
    let mut settings: AppSettings =
        serde_json::from_str(r#"{"memoryEnabled":true,"future":7}"#).unwrap();
    assert_eq!(settings.memory_enabled, Some(true));
    assert!(!settings.extra.contains_key("memoryEnabled"));
    PreferenceChange::MemoryEnabled(false).apply(&mut settings);
    let json = serde_json::to_value(settings).unwrap();
    assert_eq!(json["memoryEnabled"], false);
    assert_eq!(json["future"], 7);
    assert!(serde_json::from_str::<AppSettings>(r#"{"memoryEnabled":"true"}"#).is_err());
}
