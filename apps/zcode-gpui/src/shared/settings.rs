//! Settings reader, writer, and desktop running guard for zcode-gpui.
//! Persists to ~/.zcode/v2/setting.json.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub enum SettingsError {
    DesktopRunning,
    Invalid(String),
    /// The existing file does not parse; writing would replace it wholesale.
    Unreadable,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DesktopRunning => {
                write!(
                    f,
                    "Cannot modify settings while ZCode desktop is running. Please change this in ZCode desktop."
                )
            }
            Self::Unreadable => write!(
                f,
                "setting.json could not be parsed; not overwriting it. Fix it in ZCode desktop."
            ),
            Self::Invalid(e) => write!(f, "Invalid preference: {e}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// Typed view of the desktop's `setting.json`. Unset fields are omitted on
/// write (never `null`), and unmodeled keys round-trip through `extra`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AppSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "localePreference"
    )]
    pub locale_preference: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "themePreference"
    )]
    pub theme_preference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "uiFontSize"
    )]
    pub ui_font_size: Option<f32>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "shortcutBindings"
    )]
    pub shortcut_bindings: Option<HashMap<String, Vec<String>>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "keepAwakeWhileRunning"
    )]
    pub keep_awake_while_running: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "memoryEnabled"
    )]
    pub memory_enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "closeToTrayOnWindows"
    )]
    pub close_to_tray_on_windows: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "recentProjects"
    )]
    pub recent_projects: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "dataBaseDir"
    )]
    pub data_base_dir: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl AppSettings {
    pub(crate) fn language_preference(&self) -> crate::shared::i18n::LocalePreference {
        // selector 与运行时必须使用同一回退规则；无效偏好不能偷偷跟随系统或旧 locale。
        self.locale_preference
            .as_deref()
            .or(self.locale.as_deref())
            .map(crate::shared::i18n::LocalePreference::parse)
            .unwrap_or_default()
    }
}

/// Resolve the user directory holding `.zcode/v2/setting.json`.
pub fn resolve_user_home_dir() -> PathBuf {
    if let Some(isolated) = crate::shared::isolation::active() {
        return isolated.home();
    }
    crate::shared::data_paths::DataPaths::resolve(|k| std::env::var(k).ok(), None).settings_home
}

pub fn settings_file_path() -> PathBuf {
    resolve_user_home_dir()
        .join(".zcode")
        .join("v2")
        .join("setting.json")
}

pub use crate::shared::desktop_lock::is_desktop_running;

/// Read settings from disk, returning defaults on any error or missing file.
pub fn load_settings() -> AppSettings {
    load_settings_from_path(&settings_file_path())
}

pub fn load_settings_from_path(path: &std::path::Path) -> AppSettings {
    if let Ok(content) = std::fs::read_to_string(path)
        && let Ok(settings) = serde_json::from_str::<AppSettings>(&content)
    {
        return settings;
    }
    AppSettings::default()
}

/// Apply `change` to the settings on disk, only while no desktop instance
/// runs. The desktop rewrites `setting.json` whole from memory
/// (`settingService.ts`), so a write while it runs is silently lost.
pub fn update_settings(change: impl FnOnce(&mut AppSettings)) -> Result<(), SettingsError> {
    update_settings_at(&settings_file_path(), is_desktop_running, change)
}

/// Read-modify-write from the file's current contents (never from a stale
/// in-memory copy), re-checking the desktop lock right before the rename.
pub fn update_settings_at(
    path: &std::path::Path,
    desktop_active: impl Fn() -> bool,
    change: impl FnOnce(&mut AppSettings),
) -> Result<(), SettingsError> {
    update_settings_checked_at(path, desktop_active, |s| {
        change(s);
        Ok(())
    })
}

pub(crate) fn update_settings_checked_at(
    path: &std::path::Path,
    desktop_active: impl Fn() -> bool,
    change: impl FnOnce(&mut AppSettings) -> Result<(), SettingsError>,
) -> Result<(), SettingsError> {
    if desktop_active() {
        return Err(SettingsError::DesktopRunning);
    }
    let mut settings = match std::fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str::<AppSettings>(&text).map_err(|_| SettingsError::Unreadable)?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
        Err(e) => return Err(e.into()),
    };
    change(&mut settings)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json_text = serde_json::to_string_pretty(&settings)?;
    let tmp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::now_v7()));
    if let Err(error) = std::fs::write(&tmp_path, json_text) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error.into());
    }
    if desktop_active() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(SettingsError::DesktopRunning);
    }
    if let Err(error) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
