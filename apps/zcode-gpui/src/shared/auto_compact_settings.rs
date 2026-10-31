//! Per-model background auto-compaction preference (`gpuiAutoCompact`,
//! SETTINGS.md "Background auto-compaction"). Keyed by `provider/model`.

use crate::shared::settings::AppSettings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const DEFAULT_MINUTES: u32 = 50;
pub(crate) const MIN_MINUTES: u32 = 5;
pub(crate) const MAX_MINUTES: u32 = 1440;

fn default_minutes() -> u32 {
    DEFAULT_MINUTES
}

/// One model's setting. A model without an entry is off with the default minutes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoCompactEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_minutes")]
    pub minutes: u32,
}

impl Default for AutoCompactEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            minutes: DEFAULT_MINUTES,
        }
    }
}

impl AutoCompactEntry {
    /// Minutes limited to `MIN_MINUTES..=MAX_MINUTES`.
    pub fn clamped(self) -> Self {
        Self {
            minutes: self.minutes.clamp(MIN_MINUTES, MAX_MINUTES),
            ..self
        }
    }
}

pub type AutoCompactMap = BTreeMap<String, AutoCompactEntry>;

/// Settings key of a model: the composer value without the `$thought` suffix.
pub(crate) fn model_key(provider: &str, model: &str) -> Option<String> {
    (!provider.is_empty() && !model.is_empty()).then(|| format!("{provider}/{model}"))
}

impl AppSettings {
    /// The committed entry for `key`, clamped; off with the default minutes when absent.
    pub(crate) fn auto_compact(&self, key: &str) -> AutoCompactEntry {
        self.gpui_auto_compact
            .as_ref()
            .and_then(|map| map.get(key))
            .copied()
            .unwrap_or_default()
            .clamped()
    }

    /// Replace one model's entry, keeping the others.
    pub(crate) fn set_auto_compact(&mut self, key: &str, entry: AutoCompactEntry) {
        self.gpui_auto_compact
            .get_or_insert_with(BTreeMap::new)
            .insert(key.to_string(), entry.clamped());
    }
}

/// The committed entry for `key` from the preference owner (off when not installed).
pub(crate) fn entry(key: &str, cx: &gpui::App) -> AutoCompactEntry {
    if !cx.has_global::<crate::shared::preferences::PreferenceOwner>() {
        return AutoCompactEntry::default();
    }
    cx.global::<crate::shared::preferences::PreferenceOwner>()
        .0
        .read(cx)
        .snapshot
        .auto_compact(key)
}

/// Queue a change of one model's entry through the preference owner.
pub(crate) fn save(key: String, entry: AutoCompactEntry, cx: &mut gpui::App) {
    crate::shared::preferences::Preferences::enqueue(
        crate::shared::preferences::PreferenceChange::AutoCompact(key, entry),
        cx,
    );
}

#[cfg(test)]
#[path = "auto_compact_settings_tests.rs"]
mod tests;
