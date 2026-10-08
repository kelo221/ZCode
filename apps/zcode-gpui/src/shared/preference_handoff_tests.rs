use super::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use super::settings::AppSettings;
use gpui::TestApp;
use std::sync::Arc;

#[test]
fn suspension_drains_admitted_changes_and_defers_recent_projects() {
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    let root = std::env::temp_dir().join(format!("gpui-handoff-{}", uuid::Uuid::now_v7()));
    let path = root.join("setting.json");
    app.update(|cx| {
        Preferences::install_at(AppSettings::default(), path.clone(), Arc::new(|| false), cx);
        Preferences::enqueue(PreferenceChange::MemoryEnabled(true), cx);
        let _drained = Preferences::suspend(cx);
        Preferences::enqueue(PreferenceChange::RecentProject("later-project".into()), cx);
        Preferences::enqueue(PreferenceChange::FontSize(18.0), cx);
    });
    for _ in 0..1000 {
        app.run_until_parked();
        if app.read(Preferences::drained) {
            break;
        }
        std::thread::yield_now();
    }
    assert!(app.read(Preferences::drained));
    let disk: AppSettings = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk.memory_enabled, Some(true));
    assert!(disk.recent_projects.is_none());
    assert!(disk.ui_font_size.is_none());
    app.update(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |s, _| {
            assert!(s.suspended);
            assert_eq!(s.deferred_count(), 1);
        });
        Preferences::resume_from(disk, cx);
    });
    for _ in 0..1000 {
        app.run_until_parked();
        if app.read(Preferences::drained) {
            break;
        }
        std::thread::yield_now();
    }
    let disk: AppSettings = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk.recent_projects.unwrap(), ["later-project"]);
    assert!(disk.ui_font_size.is_none());
}

#[test]
fn deferred_patch_keeps_effective_appearance_without_restoring_stale_disk_fields() {
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    let root =
        std::env::temp_dir().join(format!("gpui-appearance-rebase-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("setting.json");
    std::fs::write(
        &path,
        r#"{"memoryEnabled":true,"syntheticUnknown":"fresh"}"#,
    )
    .unwrap();
    app.update(|cx| {
        Preferences::install_at(
            AppSettings {
                theme_preference: Some("zai-light".into()),
                ui_font_size: Some(17.0),
                ..Default::default()
            },
            path.clone(),
            Arc::new(|| false),
            cx,
        );
        let _drained = Preferences::suspend(cx);
        Preferences::enqueue(PreferenceChange::RecentProject("after-host".into()), cx);
        Preferences::resume_from(super::preference_handoff::reload(&path).unwrap(), cx);
    });
    for _ in 0..1000 {
        app.run_until_parked();
        if app.read(Preferences::drained) {
            break;
        }
        std::thread::yield_now();
    }
    assert!(app.read(Preferences::drained));
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert_eq!(
            owner.snapshot.theme_preference.as_deref(),
            Some("zai-light")
        );
        assert_eq!(owner.snapshot.ui_font_size, Some(17.0));
        assert_eq!(owner.snapshot.memory_enabled, Some(true));
    });
    let disk: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(disk["syntheticUnknown"], "fresh");
    assert_eq!(disk["recentProjects"], serde_json::json!(["after-host"]));
    assert!(disk.get("themePreference").is_none());
    assert!(disk.get("uiFontSize").is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reload_rebases_disk_but_preserves_effective_native_appearance_when_omitted() {
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    app.update(|cx| {
        Preferences::install_at(
            AppSettings {
                theme_preference: Some("zai-light".into()),
                ui_font_size: Some(17.0),
                ..Default::default()
            },
            "unused.json".into(),
            Arc::new(|| false),
            cx,
        );
        let _drained = Preferences::suspend(cx);
        Preferences::resume_from(
            AppSettings {
                memory_enabled: Some(true),
                ..Default::default()
            },
            cx,
        );
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert!(!owner.suspended);
        assert_eq!(owner.snapshot.memory_enabled, Some(true));
        assert_eq!(
            owner.snapshot.theme_preference.as_deref(),
            Some("zai-light")
        );
        assert_eq!(owner.snapshot.ui_font_size, Some(17.0));
    });
}
