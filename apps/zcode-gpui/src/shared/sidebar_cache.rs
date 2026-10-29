//! Last known sidebar rows per workspace and the open project folders, so the
//! sidebar is filled at startup before any agent has sent its list (PARITY.md
//! "Cached rows and warm agents"). The agent's sessions-index stays the fact:
//! cached rows only seed a workspace whose live list hasn't arrived and are
//! replaced by its first snapshot. GPUI-owned file, never the Desktop
//! preference file.

use crate::conversation::model::SessionEntry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

/// Rows kept per workspace: the sidebar's extended step.
pub(crate) const CACHED_ROWS: usize = 25;

pub(crate) fn path() -> PathBuf {
    crate::shared::data_paths::paths()
        .settings_home
        .join(".zcode")
        .join("v2")
        .join("gpui_sidebar_cache.json")
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedRow {
    session_id: String,
    title: String,
    #[serde(default)]
    preview: String,
    #[serde(default)]
    last_activity_at: Option<i64>,
    #[serde(default)]
    sort_activity_at: Option<i64>,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    #[serde(default)]
    rows: HashMap<String, Vec<CachedRow>>,
    #[serde(default)]
    open_folders: BTreeSet<String>,
}

#[derive(Default)]
pub(crate) struct SidebarCache {
    /// `None` keeps the cache in memory only (tests, phone build).
    path: Option<PathBuf>,
    data: Data,
    dirty: bool,
}

impl SidebarCache {
    pub(crate) fn load(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            data,
            dirty: false,
        }
    }

    /// Cached rows of `key`, newest first, shown as not running.
    pub(crate) fn rows(&self, key: &str) -> Vec<SessionEntry> {
        self.data.rows.get(key).map_or_else(Vec::new, |rows| {
            rows.iter()
                .map(|r| SessionEntry {
                    session_id: r.session_id.clone(),
                    title: r.title.clone(),
                    phase: "idle".into(),
                    last_activity_at: r.last_activity_at,
                    sort_activity_at: r.sort_activity_at,
                    preview: r.preview.clone(),
                    has_background_work: false,
                })
                .collect()
        })
    }

    /// Remember the newest rows of a live list (already sorted newest first).
    pub(crate) fn record(&mut self, key: &str, sessions: &[SessionEntry]) {
        let rows: Vec<CachedRow> = sessions
            .iter()
            .take(CACHED_ROWS)
            .map(|s| CachedRow {
                session_id: s.session_id.clone(),
                title: s.title.clone(),
                preview: crate::conversation::model::format_preview(&s.preview, 160),
                last_activity_at: s.last_activity_at,
                sort_activity_at: s.sort_activity_at,
            })
            .collect();
        if self.data.rows.get(key) != Some(&rows) {
            self.data.rows.insert(key.to_string(), rows);
            self.dirty = true;
        }
    }

    /// Forget a workspace removed from the sidebar.
    pub(crate) fn forget(&mut self, key: &str) {
        let removed = self.data.rows.remove(key).is_some();
        if removed || self.data.open_folders.remove(key) {
            self.dirty = true;
        }
    }

    pub(crate) fn open_folders(&self) -> HashSet<String> {
        self.data.open_folders.iter().cloned().collect()
    }

    pub(crate) fn set_open_folders(&mut self, open: &HashSet<String>) {
        if self.data.open_folders.len() != open.len()
            || !open.iter().all(|k| self.data.open_folders.contains(k))
        {
            self.data.open_folders = open.iter().cloned().collect();
            self.dirty = true;
        }
    }

    /// The pending write, if anything changed since the last one.
    pub(crate) fn take_write(&mut self) -> Option<(PathBuf, String)> {
        if !self.dirty {
            return None;
        }
        let path = self.path.clone()?;
        let text = serde_json::to_string(&self.data).ok()?;
        self.dirty = false;
        Some((path, text))
    }
}

#[cfg(test)]
#[path = "sidebar_cache_tests.rs"]
mod tests;
