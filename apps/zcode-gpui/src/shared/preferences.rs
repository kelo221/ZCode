use crate::shared::i18n::LocalePreference;
use crate::shared::settings::AppSettings;
use crate::shared::theme::ThemeMode;
use gpui::{App, AppContext, Context, Entity, Global};
use std::collections::{HashMap, VecDeque};

pub(crate) fn merge_recent_projects(existing: &[String], path: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = existing
        .iter()
        .filter(|p| p.as_str() != path)
        .cloned()
        .collect();
    out.insert(0, path.to_string());
    out.truncate(max);
    out
}

#[derive(Clone, Debug)]
pub(crate) enum PreferenceChange {
    Locale(LocalePreference),
    Theme(ThemeMode),
    FontSize(f32),
    MemoryEnabled(bool),
    Shortcut(String, Option<Vec<String>>),
    RecentProject(String),
}

impl PreferenceChange {
    pub fn apply(&self, settings: &mut AppSettings) {
        match self {
            Self::Locale(pref) => {
                settings.locale_preference = Some(pref.as_str().into());
                settings.locale = Some(pref.resolve().as_str().into());
            }
            Self::Theme(mode) => settings.theme_preference = Some(mode.as_str().into()),
            Self::FontSize(size) => settings.ui_font_size = Some(*size),
            Self::MemoryEnabled(enabled) => settings.memory_enabled = Some(*enabled),
            Self::Shortcut(id, bindings) => {
                let overrides = settings.shortcut_bindings.get_or_insert_with(HashMap::new);
                if let Some(bindings) = bindings {
                    overrides.insert(id.clone(), bindings.clone());
                } else {
                    overrides.remove(id);
                }
            }
            Self::RecentProject(path) => {
                settings.recent_projects = Some(merge_recent_projects(
                    settings.recent_projects.as_deref().unwrap_or(&[]),
                    path,
                    8,
                ));
            }
        }
    }

    pub fn validate(&self, settings: &AppSettings) -> Result<(), String> {
        match self {
            Self::FontSize(size) if !size.is_finite() || !(12.0..=20.0).contains(size) => {
                Err("Interface font size must be between 12 and 20".into())
            }
            Self::Shortcut(_, _) => {
                let mut candidate = settings.clone();
                self.apply(&mut candidate);
                crate::shared::shortcut_runtime::validate(candidate.shortcut_bindings.as_ref())
            }
            _ => Ok(()),
        }
    }
}

pub(crate) fn persist_change(
    path: &std::path::Path,
    desktop_active: impl Fn() -> bool,
    change: PreferenceChange,
) -> Result<AppSettings, crate::shared::settings::SettingsError> {
    let mut committed = None;
    crate::shared::settings::update_settings_checked_at(path, desktop_active, |settings| {
        // 排队后磁盘内容可能已变；在唯一写入边界复验，防止快捷键冲突覆盖。
        change
            .validate(settings)
            .map_err(crate::shared::settings::SettingsError::Invalid)?;
        change.apply(settings);
        committed = Some(settings.clone());
        Ok(())
    })?;
    Ok(committed.expect("settings change applied"))
}

pub(crate) struct Preferences {
    pub snapshot: AppSettings,
    pub saving: bool,
    pub error: Option<String>,
    queue: VecDeque<PreferenceChange>,
    path: std::path::PathBuf,
    desktop_active: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
}

pub(crate) struct PreferenceOwner(pub Entity<Preferences>);
impl Global for PreferenceOwner {}

impl Preferences {
    pub fn install(settings: AppSettings, cx: &mut App) {
        Self::install_at(
            settings,
            crate::shared::settings::settings_file_path(),
            std::sync::Arc::new(crate::shared::settings::is_desktop_running),
            cx,
        );
    }

    pub(crate) fn install_at(
        settings: AppSettings,
        path: std::path::PathBuf,
        desktop_active: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
        cx: &mut App,
    ) {
        let owner = cx.new(|_| Self {
            snapshot: settings,
            saving: false,
            error: None,
            queue: VecDeque::new(),
            path,
            desktop_active,
        });
        cx.set_global(PreferenceOwner(owner));
        Self::apply_runtime(cx);
    }

    pub fn enqueue(change: PreferenceChange, cx: &mut App) {
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |this, cx| {
            if let Err(error) = change.validate(&this.snapshot) {
                this.error = Some(error);
                cx.notify();
                return;
            }
            this.queue.push_back(change);
            if !this.saving {
                this.start_next(cx);
            }
            cx.notify();
        });
    }

    fn start_next(&mut self, cx: &mut Context<Self>) {
        let Some(change) = self.queue.pop_front() else {
            self.saving = false;
            return;
        };
        self.saving = true;
        let path = self.path.clone();
        let desktop_active = self.desktop_active.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { persist_change(&path, || desktop_active(), change) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(snapshot) => {
                        this.error = None;
                        this.snapshot = snapshot;
                        Self::apply_snapshot(&this.snapshot, cx);
                    }
                    Err(error) => {
                        this.error = Some(crate::shared::redact::scrub(&error.to_string()))
                    }
                }
                this.start_next(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn apply_runtime(cx: &mut App) {
        let settings = cx.global::<PreferenceOwner>().0.read(cx).snapshot.clone();
        Self::apply_snapshot(&settings, cx);
    }

    fn apply_snapshot(settings: &AppSettings, cx: &mut App) {
        let pref = LocalePreference::parse(
            settings
                .locale_preference
                .as_deref()
                .or(settings.locale.as_deref())
                .unwrap_or("system"),
        );
        crate::shared::i18n::set_current_locale(pref.resolve());
        let mode = ThemeMode::parse(
            settings
                .theme_preference
                .as_deref()
                .or(settings.theme.as_deref())
                .unwrap_or("system"),
        );
        crate::shared::theme::set_theme_mode(mode);
        crate::shared::theme::apply_appearance(cx.window_appearance(), cx);
        crate::shared::theme::set_ui_font_size(settings.ui_font_size.unwrap_or(14.0));
        ely_gpui_component::theme::Theme::update(cx, |theme| {
            theme.font_scale = crate::shared::theme::font_size_base() / 14.0;
        });
        cx.refresh_windows();
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
