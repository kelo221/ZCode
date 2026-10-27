//! Newest task activity seen per project, so the sidebar can order projects
//! whose agent hasn't started this run (PARITY.md "Project order"). A project's
//! task list only arrives once its agent runs, so without this every project
//! except the startup one sank to discovery order until it was opened.
//! Stored in its own GPUI-owned file, never in the preference file Desktop writes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) fn path() -> PathBuf {
    crate::shared::data_paths::paths()
        .settings_home
        .join(".zcode")
        .join("v2")
        .join("gpui_project_activity.json")
}

/// `{ canonical workspace key: newest lastActivityAt (ms) }`.
#[derive(Default)]
pub(crate) struct ProjectActivity {
    /// `None` keeps the store in memory only (tests, phone build).
    path: Option<PathBuf>,
    latest: HashMap<String, i64>,
    dirty: bool,
}

impl ProjectActivity {
    pub(crate) fn load(path: PathBuf) -> Self {
        let latest = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            latest,
            dirty: false,
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<i64> {
        self.latest.get(key).copied()
    }

    /// Only moves forward: an older value (a stale snapshot, a deleted task)
    /// never pulls a project down below activity already seen.
    pub(crate) fn record(&mut self, key: &str, at: Option<i64>) {
        let Some(at) = at else { return };
        if self.latest.get(key).is_some_and(|&prev| prev >= at) {
            return;
        }
        self.latest.insert(key.to_string(), at);
        self.dirty = true;
    }

    /// The pending write, if anything changed since the last one.
    pub(crate) fn take_write(&mut self) -> Option<(PathBuf, String)> {
        if !self.dirty {
            return None;
        }
        let path = self.path.clone()?;
        let text = serde_json::to_string(&self.latest).ok()?;
        self.dirty = false;
        Some((path, text))
    }
}

pub(crate) fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_only_newer_activity_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("zcode_proj_act_{}", std::process::id()));
        let file = dir.join("gpui_project_activity.json");
        let mut store = ProjectActivity::load(file.clone());
        assert_eq!(store.get("a"), None);
        assert!(store.take_write().is_none());

        store.record("a", Some(200));
        store.record("a", Some(100));
        store.record("b", None);
        assert_eq!(store.get("a"), Some(200));
        assert_eq!(store.get("b"), None);

        let (path, text) = store.take_write().expect("dirty after a newer value");
        assert!(store.take_write().is_none());
        write(&path, &text).unwrap();
        store.record("a", Some(150));
        assert!(store.take_write().is_none(), "older value is not a change");

        assert_eq!(ProjectActivity::load(file).get("a"), Some(200));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_or_corrupt_file_starts_empty() {
        let dir = std::env::temp_dir().join(format!("zcode_proj_act_bad_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("gpui_project_activity.json");
        std::fs::write(&file, "not json").unwrap();
        assert_eq!(ProjectActivity::load(file).get("a"), None);
        assert_eq!(ProjectActivity::default().take_write(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
