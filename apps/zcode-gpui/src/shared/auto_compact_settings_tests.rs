//! Tests for `shared/auto_compact_settings.rs`.

use super::*;
use crate::shared::preferences::{PreferenceChange, persist_change};

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("zcode_auto_compact_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn missing_entry_is_off_with_default_minutes() {
    let settings = AppSettings::default();
    assert_eq!(
        settings.auto_compact("p/m"),
        AutoCompactEntry {
            enabled: false,
            minutes: 50
        }
    );
}

#[test]
fn minutes_are_clamped_on_read_and_write() {
    let mut settings = AppSettings::default();
    settings.set_auto_compact(
        "p/m",
        AutoCompactEntry {
            enabled: true,
            minutes: 1,
        },
    );
    assert_eq!(settings.auto_compact("p/m").minutes, MIN_MINUTES);
    // A hand-edited file can hold anything; reads clamp too.
    settings.gpui_auto_compact.as_mut().unwrap().insert(
        "p/big".into(),
        AutoCompactEntry {
            enabled: true,
            minutes: 99_999,
        },
    );
    assert_eq!(settings.auto_compact("p/big").minutes, MAX_MINUTES);
}

#[test]
fn entry_round_trips_through_the_gpui_file_only() {
    let dir = scratch("roundtrip");
    let file = dir.join("setting.json");
    std::fs::write(&file, r#"{"locale":"en-US"}"#).unwrap();
    let entry = AutoCompactEntry {
        enabled: true,
        minutes: 30,
    };
    // Saves while the desktop runs: GPUI-only fields never touch setting.json.
    persist_change(
        &file,
        || true,
        PreferenceChange::AutoCompact("p/a".into(), entry),
    )
    .unwrap();
    persist_change(
        &file,
        || true,
        PreferenceChange::AutoCompact("p/b".into(), AutoCompactEntry::default()),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"locale":"en-US"}"#
    );
    let raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(crate::shared::settings::gpui_settings_path(&file)).unwrap(),
    )
    .unwrap();
    assert_eq!(raw["gpuiAutoCompact"]["p/a"]["enabled"], true);
    assert_eq!(raw["gpuiAutoCompact"]["p/a"]["minutes"], 30);
    let loaded = crate::shared::settings::load_settings_from_path(&file);
    assert_eq!(loaded.auto_compact("p/a"), entry);
    assert_eq!(loaded.auto_compact("p/b"), AutoCompactEntry::default());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn partial_entry_defaults_minutes() {
    let map: AutoCompactMap = serde_json::from_str(r#"{"p/m":{"enabled":true}}"#).unwrap();
    assert_eq!(map["p/m"].minutes, DEFAULT_MINUTES);
}

#[test]
fn key_without_provider_or_model_is_rejected() {
    let settings = AppSettings::default();
    for key in ["", "model", "/m", "p/"] {
        let change = PreferenceChange::AutoCompact(key.into(), AutoCompactEntry::default());
        assert!(change.validate(&settings).is_err(), "{key}");
    }
    assert_eq!(model_key("p", "m").as_deref(), Some("p/m"));
    assert_eq!(model_key("", "m"), None);
}
