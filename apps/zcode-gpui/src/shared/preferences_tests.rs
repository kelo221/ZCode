use super::*;
use crate::shared::settings::SettingsError;

#[test]
fn read_only_owner_refuses_before_queue_or_disk_write() {
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut app = gpui::TestApp::with_text_system_and_assets(
        std::sync::Arc::new(gpui::NoopTextSystem::new()),
        std::sync::Arc::new(ely_gpui_component::Assets),
    );
    let dir = std::env::temp_dir().join(format!("gpui-readonly-{}", uuid::Uuid::now_v7()));
    let path = dir.join("setting.json");
    app.update(|cx| {
        ely_gpui_component::init(cx);
        Preferences::install_at(
            AppSettings::default(),
            path.clone(),
            std::sync::Arc::new(|| false),
            cx,
        );
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |s, _| s.read_only = true);
        Preferences::enqueue(PreferenceChange::MemoryEnabled(true), cx);
        let owner = owner.read(cx);
        assert!(owner.queue.is_empty());
        assert!(!owner.saving);
        assert_eq!(owner.snapshot.memory_enabled, None);
        assert!(owner.error.as_ref().unwrap().contains("read-only"));
    });
    assert!(!path.exists());
}

#[test]
fn disk_boundary_revalidates_against_latest_shortcuts() {
    let dir = std::env::temp_dir().join(format!("gpui-pref-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("setting.json");
    let stale = AppSettings::default();
    let change = PreferenceChange::Shortcut("openSettings".into(), Some(vec!["Ctrl+q".into()]));
    assert!(change.validate(&stale).is_ok());
    std::fs::write(
        &file,
        r#"{"shortcutBindings":{"openCommandCenter":["Ctrl+q"]},"unmodeled":true}"#,
    )
    .unwrap();
    let before = std::fs::read_to_string(&file).unwrap();
    let result = persist_change(&file, || false, change);
    assert!(matches!(result, Err(SettingsError::Invalid(_))));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn consecutive_mutations_preserve_external_fields_and_recent_projects() {
    let dir = std::env::temp_dir().join(format!("gpui-pref-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"unmodeled":{"kept":true}}"#).unwrap();
    persist_change(&file, || false, PreferenceChange::FontSize(18.0)).unwrap();
    let committed = persist_change(
        &file,
        || false,
        PreferenceChange::RecentProject("project".into()),
    )
    .unwrap();
    assert_eq!(committed.ui_font_size, Some(18.0));
    assert_eq!(committed.recent_projects.unwrap(), ["project"]);
    assert_eq!(committed.extra["unmodeled"]["kept"], true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mutations_preserve_unknown_fields_and_other_preferences() {
    let mut settings: AppSettings =
        serde_json::from_str(r#"{"custom":{"kept":true},"locale":"en-US"}"#).unwrap();
    PreferenceChange::FontSize(18.0).apply(&mut settings);
    PreferenceChange::RecentProject("project".into()).apply(&mut settings);
    assert_eq!(settings.ui_font_size, Some(18.0));
    assert_eq!(settings.locale.as_deref(), Some("en-US"));
    assert_eq!(settings.extra["custom"]["kept"], true);
    assert_eq!(settings.recent_projects.unwrap(), ["project"]);
}

#[test]
fn opened_project_joins_the_full_project_list_once() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{"lastWorkspaceSession":[{"kind":"remote","workspacePath":"F:\\p"}]}"#,
    )
    .unwrap();
    PreferenceChange::RecentProject("F:\\p".into()).apply(&mut settings);
    PreferenceChange::RecentProject("F:\\p".into()).apply(&mut settings);
    let list = settings.extra["lastWorkspaceSession"].as_array().unwrap();
    assert_eq!(
        list.len(),
        2,
        "the remote entry stays, one local entry is added"
    );
    assert_eq!(list[1]["kind"], "local");
    assert_eq!(list[1]["workspacePath"], "F:\\p");
    assert_eq!(list[1]["workspacePurpose"], "project");
}

#[test]
fn recent_projects_keep_the_desktop_limit_of_ten() {
    let mut settings = AppSettings::default();
    for i in 0..12 {
        PreferenceChange::RecentProject(format!("p{i}")).apply(&mut settings);
    }
    let recents = settings.recent_projects.unwrap();
    assert_eq!(recents.len(), 10);
    assert_eq!(recents[0], "p11");
    let list = settings.extra["lastWorkspaceSession"].as_array().unwrap();
    assert_eq!(list.len(), 12, "the full list is not capped");
}

#[test]
fn removed_project_is_forgotten_everywhere_and_other_entries_stay() {
    let mut settings: AppSettings = serde_json::from_str(
        r#"{
            "recentProjects": ["gone", "kept"],
            "lastWorkspaceSession": [
                {"kind":"local","workspacePath":"gone","workspacePurpose":"project"},
                {"kind":"remote","workspacePath":"gone"},
                {"kind":"local","workspacePath":"kept","workspacePurpose":"project"}
            ],
            "lastActiveWorkspace": {"kind":"local","workspacePath":"gone","workspacePurpose":"project"},
            "custom": 1
        }"#,
    )
    .unwrap();
    PreferenceChange::RemoveProject("gone".into()).apply(&mut settings);
    assert_eq!(settings.recent_projects.unwrap(), ["kept"]);
    let list = settings.extra["lastWorkspaceSession"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        list[0]["kind"], "remote",
        "remote entries are another host's"
    );
    assert_eq!(list[1]["workspacePath"], "kept");
    assert!(settings.extra["lastActiveWorkspace"].is_null());
    assert_eq!(settings.extra["custom"], 1);
}

#[test]
fn removing_a_project_keeps_a_different_last_active_one() {
    let mut settings: AppSettings =
        serde_json::from_str(r#"{"lastActiveWorkspace":{"kind":"local","workspacePath":"kept"}}"#)
            .unwrap();
    PreferenceChange::RemoveProject("gone".into()).apply(&mut settings);
    assert_eq!(
        settings.extra["lastActiveWorkspace"]["workspacePath"],
        "kept"
    );
}

#[test]
fn invalid_size_is_rejected_before_commit() {
    let settings = AppSettings::default();
    for size in [f32::NAN, f32::INFINITY, 11.0, 21.0] {
        assert!(
            PreferenceChange::FontSize(size)
                .validate(&settings)
                .is_err()
        );
    }
    assert!(PreferenceChange::FontSize(16.0).validate(&settings).is_ok());
}

#[test]
fn shortcut_reset_does_not_delete_unknown_command_overrides() {
    let mut settings = AppSettings {
        shortcut_bindings: Some(HashMap::from([(
            "futureCommand".into(),
            vec!["Ctrl+x".into()],
        )])),
        ..Default::default()
    };
    PreferenceChange::Shortcut("openSettings".into(), Some(vec![])).apply(&mut settings);
    PreferenceChange::Shortcut("openSettings".into(), None).apply(&mut settings);
    assert_eq!(settings.shortcut_bindings.unwrap().len(), 1);
}
