//! Tests for `shared/settings.rs`.

use super::*;

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("zcode_settings_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn write_is_refused_while_desktop_runs() {
    let dir = scratch("locked");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    let err = update_settings_at(&file, || true, |s| s.locale = Some("zh-CN".into())).unwrap_err();
    assert!(matches!(err, SettingsError::DesktopRunning));
    assert!(err.to_string().contains("change this in ZCode desktop"));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"locale":"en-US"}"#
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn desktop_starting_mid_write_aborts_before_rename() {
    let dir = scratch("race");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    let calls = std::cell::Cell::new(0);
    let active = || {
        calls.set(calls.get() + 1);
        calls.get() > 1
    };
    let err = update_settings_at(&file, active, |s| s.locale = Some("zh-CN".into())).unwrap_err();
    assert!(matches!(err, SettingsError::DesktopRunning));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"locale":"en-US"}"#
    );
    let leftovers = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(leftovers, 1, "temp file cleaned up");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_merges_into_current_file_and_preserves_unknown_keys() {
    let dir = scratch("merge");
    let file = dir.join("setting.json");
    std::fs::write(
        &file,
        r#"{ "locale": "en-US", "customUnmodeledKey": "kept", "nestedObject": { "flag": true } }"#,
    )
    .unwrap();
    update_settings_at(&file, || false, |s| s.ui_font_size = Some(16.0)).unwrap();
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(raw["locale"], "en-US");
    assert_eq!(raw["uiFontSize"], 16.0);
    assert_eq!(raw["customUnmodeledKey"], "kept");
    assert_eq!(raw["nestedObject"]["flag"], true);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unset_fields_are_omitted_not_null() {
    let settings = AppSettings {
        locale: Some("zh-CN".to_string()),
        ..Default::default()
    };
    let raw = serde_json::to_value(&settings).unwrap();
    let obj = raw.as_object().unwrap();
    assert_eq!(obj.len(), 1, "only set keys are written: {raw}");
    assert!(obj.values().all(|v| !v.is_null()));
}

#[test]
fn unparseable_file_is_never_overwritten() {
    let dir = scratch("corrupt");
    let file = dir.join("setting.json");
    std::fs::write(&file, "{ not json").unwrap();
    let err = update_settings_at(&file, || false, |s| s.locale = Some("x".into())).unwrap_err();
    assert!(matches!(err, SettingsError::Unreadable));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_replacement_cleans_temporary_file() {
    let dir = scratch("replace-failed");
    let file = dir.join("setting.json");
    let result = update_settings_at(
        &file,
        || false,
        |s| {
            s.ui_font_size = Some(16.0);
            std::fs::create_dir(&file).unwrap();
        },
    );
    assert!(matches!(result, Err(SettingsError::Io(_))));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(file.is_dir());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_file_is_created() {
    let dir = scratch("fresh");
    let file = dir.join("v2").join("setting.json");
    update_settings_at(&file, || false, |s| s.locale = Some("zh-CN".into())).unwrap();
    assert_eq!(
        load_settings_from_path(&file).locale.as_deref(),
        Some("zh-CN")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
