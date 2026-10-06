use super::*;
use crate::shared::settings::SettingsError;

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
