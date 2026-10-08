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
    pub read_only: bool,
    pub suspended: bool,
    pub(super) queue: VecDeque<PreferenceChange>,
    pub(super) deferred: VecDeque<PreferenceChange>,
    pub(super) drain_waiters: Vec<futures::channel::oneshot::Sender<()>>,
    pub(super) path: std::path::PathBuf,
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
            read_only: crate::shared::isolation::active().is_some(),
            suspended: false,
            queue: VecDeque::new(),
            deferred: VecDeque::new(),
            drain_waiters: Vec::new(),
            path,
            desktop_active,
        });
        cx.set_global(PreferenceOwner(owner));
        Self::apply_runtime(cx);
    }

    pub fn enqueue(change: PreferenceChange, cx: &mut App) {
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |this, cx| {
            if this.read_only {
                this.error =
                    Some("Application preferences are read-only in isolated Settings mode".into());
                cx.notify();
                return;
            }
            if this.suspended {
                if matches!(change, PreferenceChange::RecentProject(_)) && this.deferred.len() < 64
                {
                    this.deferred.push_back(change);
                } else {
                    this.error = Some(
                        "Close Subagents manager before saving application preferences".into(),
                    );
                }
                cx.notify();
                return;
            }
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

    pub(super) fn start_next(&mut self, cx: &mut Context<Self>) {
        let Some(change) = self.queue.pop_front() else {
            self.saving = false;
            for waiter in self.drain_waiters.drain(..) {
                let _ = waiter.send(());
            }
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
                    Ok(mut snapshot) => {
                        // Host 会移除原生外观字段；后续 recentProjects 写入只更新磁盘补丁，不丢失当前有效外观。
                        if snapshot.theme_preference.is_none() {
                            snapshot.theme_preference = this.snapshot.theme_preference.clone();
                        }
                        if snapshot.ui_font_size.is_none() {
                            snapshot.ui_font_size = this.snapshot.ui_font_size;
                        }
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

    pub(super) fn apply_snapshot(settings: &AppSettings, cx: &mut App) {
        crate::shared::i18n::apply_locale(settings.language_preference().resolve(), cx);
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
        crate::shared::acceptance::apply_runtime(cx);
        cx.refresh_windows();
    }
}

#[cfg(test)]
#[path = "preferences_tests.rs"]
mod tests;
