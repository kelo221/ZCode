//! Exact file-name lookup over the `@` index (transcript file references).

use super::{FileHit, registry, walk};
use std::path::Path;

/// How many files under `root` are named `name`, from the `@` index only; `None`
/// while it is not built yet (this starts the build). Never walks: safe on the UI thread.
pub(crate) fn count_named_if_indexed(root: &Path, name: &str) -> Option<usize> {
    let built = {
        let reg = registry().lock().unwrap_or_else(|e| e.into_inner());
        reg.get(root).map(|i| {
            i.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .clone()
        })
    };
    match built {
        Some(Some(entries)) => Some(
            entries
                .iter()
                .filter(|e| !e.hit.is_dir && e.hit.name.eq_ignore_ascii_case(name))
                .count(),
        ),
        Some(None) => None,
        None => {
            super::search(root, "", 0);
            None
        }
    }
}

/// Root-relative paths of the files under `root` named `name` (ASCII case-insensitive).
/// Uses the `@` index when it is built, else walks the tree; call off the UI thread.
pub(crate) fn files_named(root: &Path, name: &str) -> Vec<String> {
    let built = {
        let reg = registry().lock().unwrap_or_else(|e| e.into_inner());
        reg.get(root).and_then(|i| {
            i.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entries
                .clone()
        })
    };
    let keep = |hit: &FileHit| !hit.is_dir && hit.name.eq_ignore_ascii_case(name);
    match built {
        Some(entries) => entries
            .iter()
            .filter(|e| keep(&e.hit))
            .map(|e| e.hit.rel.clone())
            .collect(),
        None => walk(root)
            .into_iter()
            .filter(|e| keep(&e.hit))
            .map(|e| e.hit.rel)
            .collect(),
    }
}
