//! Exact file-name lookup over the `@` index (transcript file references).

use super::{FileHit, registry, walk};
use std::path::Path;

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
