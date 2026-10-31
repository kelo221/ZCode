//! Whether inline-code file references point at a real file, so only those
//! render as links (PARITY.md "Transcript file references"). Markdown
//! re-renders every frame, so answers are cached per path for the active
//! workspace; the transcript sets that workspace before rendering rows.

use super::file_ref::{is_bare_name, resolve_path};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A found file stays linked this long before it is checked again.
const FOUND_TTL: Duration = Duration::from_secs(60);
/// A missing file may be created by the agent soon; look again sooner.
const MISSING_TTL: Duration = Duration::from_secs(5);
const MAX_CACHED: usize = 4096;

#[derive(Default)]
struct Oracle {
    root: Option<PathBuf>,
    home: Option<PathBuf>,
    cache: HashMap<String, (bool, Instant)>,
}

thread_local! {
    static ORACLE: RefCell<Oracle> = RefCell::new(Oracle::default());
}

/// Workspace that file references in the next rendered Markdown resolve against.
pub(crate) fn set_root(root: Option<PathBuf>) {
    ORACLE.with_borrow_mut(|o| {
        if o.root != root {
            o.root = root;
            o.cache.clear();
        }
    });
}

/// `Some(false)` only when the path is known not to resolve to a file; `None`
/// (no workspace yet, or the workspace index is still building) keeps the link.
pub(crate) fn file_exists(path: &str) -> Option<bool> {
    ORACLE.with_borrow_mut(|o| {
        let root = o.root.clone()?;
        if let Some((found, at)) = o.cache.get(path) {
            let ttl = if *found { FOUND_TTL } else { MISSING_TTL };
            if at.elapsed() < ttl {
                return Some(*found);
            }
        }
        let home = o
            .home
            .get_or_insert_with(crate::shared::settings::resolve_user_home_dir)
            .clone();
        let found = check(&root, Some(&home), path)?;
        if o.cache.len() >= MAX_CACHED {
            o.cache.clear();
        }
        o.cache.insert(path.to_string(), (found, Instant::now()));
        Some(found)
    })
}

/// Stricter than the click-time lookup (`files::file_reference::locate`): the
/// resolved path is an existing file or folder, or a bare name matches exactly
/// one workspace file. Missing folders are never linked from inline code.
fn check(root: &Path, home: Option<&Path>, path: &str) -> Option<bool> {
    if resolve_path(Some(root), home, path).is_some_and(|p| p.exists()) {
        return Some(true);
    }
    if !is_bare_name(path) {
        return Some(false);
    }
    crate::composer::file_index::count_named_if_indexed(root, path).map(|n| n == 1)
}

#[cfg(test)]
#[path = "file_exists_tests.rs"]
mod tests;
