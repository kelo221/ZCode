use crate::app::{
    profile_state::{ProfileMutation, ProfileRequest},
    root::RootView,
    settings::SettingsSection,
    store::AppState,
    test_support::*,
};
use crate::shared::{
    preferences::{PreferenceOwner, Preferences},
    settings::AppSettings,
};
use gpui::{MouseButton, TestApp};
use serde_json::json;
use std::sync::Arc;

fn fixture() -> (TestApp, gpui::Entity<AppState>, std::path::PathBuf) {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let path = std::env::temp_dir()
        .join(format!("gpui-manager-test-{}", uuid::Uuid::now_v7()))
        .join("setting.json");
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(AppSettings::default(), path.clone(), Arc::new(|| false), cx);
    });
    let state = app.new_entity(AppState::for_test);
    (app, state, path)
}

#[test]
fn activation_disclosure_and_cancel_do_not_launch_or_suspend_preferences() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, path) = fixture();
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings_section(SettingsSection::Subagents, window, cx);
        view
    });
    window.draw();
    let at = app.read(|cx| cx.global::<TestTargets>().0["subagents-activate"].center());
    window.simulate_click(at, MouseButton::Left);
    window.draw();
    window.read(|view, _| assert!(view.subagents.activation_confirm));
    let at = app.read(|cx| cx.global::<TestTargets>().0["subagents-activate-cancel"].center());
    window.simulate_click(at, MouseButton::Left);
    app.read_entity(&state, |s, cx| {
        assert!(!s.profiles.local_enabled);
        assert!(!s.profiles.starting);
        assert!(s.profiles.client.is_none());
        assert!(!cx.global::<PreferenceOwner>().0.read(cx).suspended);
    });
    assert!(!path.exists());
}

#[test]
fn close_waits_for_admitted_work_then_reloads_without_replay() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, path) = fixture();
    app.update_entity(&state, |s, cx| {
        let _drain = Preferences::suspend(cx);
        s.profiles.local_enabled = true;
        s.profiles.mutation_pending = true;
        s.close_profiles(cx);
        assert!(s.profiles.closing);
        assert!(!s.profiles.stopping);
        s.submit_profile_mutation(
            "later".into(),
            "user".into(),
            "createAgent",
            json!({}),
            None,
            cx,
        );
        assert!(s.profiles.queue.is_empty());
        let m = ProfileMutation {
            origin: Default::default(),
            receipt: "admitted".into(),
            scope: "user".into(),
            method: "deleteAgent".into(),
            params: json!({}),
            baseline: None,
        };
        s.settle_profile_request(ProfileRequest::Commit(m), Err("lost response".into()), cx);
        assert!(s.profiles.stopping);
        assert!(s.profiles.uncertain);
        assert!(cx.global::<PreferenceOwner>().0.read(cx).suspended);
    });
    for _ in 0..10000 {
        app.run_until_parked();
        if app.read_entity(&state, |s, _| !s.profiles.closing) {
            break;
        }
        std::thread::yield_now();
    }
    app.read_entity(&state, |s, cx| {
        assert!(!s.profiles.local_enabled);
        assert!(!s.profiles.closing);
        assert!(!cx.global::<PreferenceOwner>().0.read(cx).suspended);
        assert!(s.profiles.queue.is_empty());
        assert!(s.profiles.recovery.is_some());
    });
    assert!(!path.exists());
}

#[test]
fn failed_startup_observer_blocks_resume_until_owned_process_is_gone() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, _) = fixture();
    let client = crate::backend::services_rpc::fake_child::spawn("normal").unwrap();
    let observer = client.exit_observer();
    app.update_entity(&state, |s, cx| {
        let _drain = Preferences::suspend(cx);
        s.profiles.local_enabled = true;
        s.profiles.startup_exit = Some(observer.clone());
        s.close_profiles(cx);
        assert!(s.profiles.stopping);
        assert!(cx.global::<PreferenceOwner>().0.read(cx).suspended);
        // TestApp 在 update 结束时执行阻塞后台 wait；必须在同一回合释放 peer，不能靠 10 秒超时推进。
        assert!(observer.wait_for_exit(std::time::Duration::ZERO).is_err());
        drop(client);
    });
    observer
        .wait_for_exit(std::time::Duration::from_secs(5))
        .unwrap();
    for _ in 0..10000 {
        app.run_until_parked();
        if app.read_entity(&state, |s, _| !s.profiles.closing) {
            break;
        }
        std::thread::yield_now();
    }
    app.read_entity(&state, |s, cx| {
        assert!(!s.profiles.closing);
        assert!(!cx.global::<PreferenceOwner>().0.read(cx).suspended);
        assert!(s.profiles.startup_exit.is_none());
    });
}

#[test]
fn owned_host_exit_is_observed_before_deferred_preferences_write() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, path) = fixture();
    let client = crate::backend::services_rpc::fake_child::spawn("normal").unwrap();
    let observer = client.exit_observer();
    app.update_entity(&state, |s, cx| {
        let _drain = Preferences::suspend(cx);
        s.profiles.local_enabled = true;
        s.profiles.client = Some(client);
        Preferences::enqueue(
            crate::shared::preferences::PreferenceChange::RecentProject("synthetic-later".into()),
            cx,
        );
        s.close_profiles(cx);
        assert!(cx.global::<PreferenceOwner>().0.read(cx).suspended);
        assert!(!path.exists());
    });
    for _ in 0..10000 {
        app.run_until_parked();
        if app.read_entity(&state, |s, cx| {
            !s.profiles.closing && !cx.global::<PreferenceOwner>().0.read(cx).saving
        }) {
            break;
        }
        std::thread::yield_now();
    }
    observer.wait_for_exit(std::time::Duration::ZERO).unwrap();
    app.read_entity(&state, |s, cx| {
        assert!(!s.profiles.local_enabled);
        assert!(!cx.global::<PreferenceOwner>().0.read(cx).suspended);
        assert!(!cx.global::<PreferenceOwner>().0.read(cx).saving);
    });
    let saved: AppSettings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved.recent_projects.unwrap(), ["synthetic-later"]);
}

#[test]
fn corrupt_reload_keeps_gate_closed_and_has_no_reconnect() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, path) = fixture();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{").unwrap();
    app.update_entity(&state, |s, cx| {
        let _drain = Preferences::suspend(cx);
        s.profiles.local_enabled = true;
        s.close_profiles(cx);
    });
    for _ in 0..10000 {
        app.run_until_parked();
        if app.read_entity(&state, |s, _| s.profiles.connection_error.is_some()) {
            break;
        }
        std::thread::yield_now();
    }
    app.read_entity(&state, |s, cx| {
        assert!(s.profiles.closing);
        assert!(!s.profiles.ready());
        assert!(cx.global::<PreferenceOwner>().0.read(cx).suspended);
        assert!(
            s.profiles
                .connection_error
                .as_ref()
                .unwrap()
                .contains("reload failed")
        );
        assert!(s.profiles.queue.is_empty());
    });
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{");
}
