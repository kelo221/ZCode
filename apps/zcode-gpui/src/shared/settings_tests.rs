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
    assert!(err.to_string().contains("change this in GCode desktop"));
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

fn voice_on() -> crate::shared::voice_settings::VoiceSettings {
    crate::shared::voice_settings::VoiceSettings {
        enabled: true,
        ..Default::default()
    }
}

fn raw_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn gpui_only_settings_survive_the_desktop_rewriting_setting_json() {
    let dir = scratch("electron-rewrite");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    update_settings_at(
        &file,
        || false,
        |s| {
            s.gpui_voice = Some(voice_on());
            s.gpui_code_search = Some(false);
        },
    )
    .unwrap();
    assert!(raw_json(&file).get("gpuiVoice").is_none());
    assert_eq!(
        raw_json(&gpui_settings_path(&file))["gpuiVoice"]["enabled"],
        true
    );

    // Electron rewrites setting.json whole from its own typed copy.
    std::fs::write(&file, r#"{"locale":"zh-CN","recentProjects":[]}"#).unwrap();
    let loaded = load_settings_from_path(&file);
    assert!(loaded.gpui_voice.is_some_and(|v| v.enabled));
    assert_eq!(loaded.gpui_code_search, Some(false));
    assert_eq!(loaded.locale.as_deref(), Some("zh-CN"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn gpui_only_changes_save_while_the_desktop_runs_without_touching_setting_json() {
    let dir = scratch("gpui-only-locked");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    update_settings_at(&file, || true, |s| s.gpui_voice = Some(voice_on())).unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"locale":"en-US"}"#
    );
    assert!(
        load_settings_from_path(&file)
            .gpui_voice
            .is_some_and(|v| v.enabled)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_shared_change_writes_neither_file() {
    let dir = scratch("mixed-locked");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    let err = update_settings_at(
        &file,
        || true,
        |s| {
            s.locale = Some("zh-CN".into());
            s.gpui_voice = Some(voice_on());
        },
    )
    .unwrap_err();
    assert!(matches!(err, SettingsError::DesktopRunning));
    assert!(!gpui_settings_path(&file).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn legacy_values_in_setting_json_are_read_and_moved_on_the_next_write() {
    let dir = scratch("legacy");
    let file = dir.join("setting.json");
    std::fs::write(
        &file,
        r#"{"locale":"en-US","gpuiVoice":{"enabled":true},"gpuiNoiseFree":false}"#,
    )
    .unwrap();
    let loaded = load_settings_from_path(&file);
    assert!(loaded.gpui_voice.is_some_and(|v| v.enabled));
    assert_eq!(loaded.gpui_noise_free, Some(false));

    update_settings_at(&file, || false, |s| s.locale = Some("zh-CN".into())).unwrap();
    let shared = raw_json(&file);
    assert!(shared.get("gpuiVoice").is_none() && shared.get("gpuiNoiseFree").is_none());
    assert_eq!(shared["locale"], "zh-CN");
    let own = raw_json(&gpui_settings_path(&file));
    assert_eq!(own["gpuiVoice"]["enabled"], true);
    assert_eq!(own["gpuiNoiseFree"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_gpui_file_wins_over_stale_legacy_values() {
    let dir = scratch("legacy-stale");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"gpuiVoice":{"enabled":false}}"#).unwrap();
    std::fs::write(
        gpui_settings_path(&file),
        r#"{"gpuiVoice":{"enabled":true}}"#,
    )
    .unwrap();
    assert!(
        load_settings_from_path(&file)
            .gpui_voice
            .is_some_and(|v| v.enabled)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unparseable_gpui_file_is_never_overwritten() {
    let dir = scratch("gpui-corrupt");
    let file = dir.join("setting.json");
    let own = gpui_settings_path(&file);
    std::fs::write(&own, "{ not json").unwrap();
    let err = update_settings_at(&file, || false, |s| s.gpui_voice = Some(voice_on())).unwrap_err();
    assert!(matches!(err, SettingsError::Unreadable));
    assert_eq!(std::fs::read_to_string(&own).unwrap(), "{ not json");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn voice_settings_round_trip_under_one_gpui_key() {
    let settings = AppSettings {
        gpui_voice: Some(crate::shared::voice_settings::VoiceSettings {
            enabled: true,
            voice: "bm_george".into(),
            speed: 1.2,
            volume: 0.5,
            only_when_unfocused: true,
        }),
        ..Default::default()
    };
    let raw = serde_json::to_value(&settings).unwrap();
    assert_eq!(raw["gpuiVoice"]["onlyWhenUnfocused"], true);
    assert_eq!(raw["gpuiVoice"]["voice"], "bm_george");
    let back: AppSettings = serde_json::from_value(raw).unwrap();
    assert_eq!(back.gpui_voice, settings.gpui_voice);
    let partial: AppSettings =
        serde_json::from_value(serde_json::json!({ "gpuiVoice": { "enabled": true } })).unwrap();
    let voice = partial.gpui_voice.unwrap();
    assert!(voice.enabled && !voice.only_when_unfocused);
    assert_eq!(voice.voice, "af_sky");
}
