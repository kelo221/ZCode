//! Lifecycle owner for temp files created from pasted clipboard images.
//!
//! The composer used to write timestamp-named files into the global temp dir
//! with no ownership tracking, so every paste leaked a file (2026-10-05 audit
//! P1.7). Files now use content-derived names under a `zcode-gpui-paste-`
//! prefix and are deleted through this module: on attachment remove, on app
//! quit (retired after submit), and via a stale sweep at startup.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Prefix identifying temp files this app owns (the sweep only ever deletes
/// files carrying it).
pub(crate) const TEMP_PREFIX: &str = "zcode-gpui-paste-";

/// Write `bytes` as a temp file named by content hash (dedupes identical
/// pastes, avoids timestamp collisions). Returns the path; the caller owns
/// deletion through [`delete_owned`].
pub(crate) fn write_temp_image(bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut name = String::with_capacity(TEMP_PREFIX.len() + 16 + ext.len() + 1);
    name.push_str(TEMP_PREFIX);
    for b in digest.iter().take(8) {
        use std::fmt::Write;
        let _ = write!(name, "{b:02x}");
    }
    name.push('.');
    name.push_str(ext.trim_start_matches('.'));
    let path = std::env::temp_dir().join(name);
    if path.exists() {
        // Same content already materialized: reuse the file.
        return Ok(path);
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        f.write_all(bytes)?;
    }
    Ok(path)
}

/// True when `reference` points at a temp file this app created (owned
/// prefix in its file name).
pub(crate) fn is_owned_path(reference: &str) -> bool {
    std::path::Path::new(reference)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(TEMP_PREFIX))
}

/// Best-effort delete of an owned temp file. Never panics; missing files are
/// fine (they may already have been consumed or swept).
pub(crate) fn delete_owned(path: &Path) {
    if is_owned_path(&path.to_string_lossy()) {
        let _ = std::fs::remove_file(path);
    }
}

/// Startup sweep: remove paste temp files older than 24h (crash leftovers).
/// Only files with the owned prefix are considered.
pub(crate) fn scavenge_stale() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let max_age = Duration::from_secs(24 * 3600);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(TEMP_PREFIX) {
            continue;
        }
        let fresh = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age < max_age);
        if !fresh {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_image_names_are_content_derived() {
        let a = write_temp_image(b"hello image", "png").unwrap();
        let b = write_temp_image(b"hello image", "png").unwrap();
        assert_eq!(a, b, "same bytes must reuse the same file");
        assert!(
            a.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(TEMP_PREFIX))
        );
        delete_owned(&a);
        assert!(!a.exists());
    }

    #[test]
    fn delete_refuses_files_without_the_owned_prefix() {
        let dir = std::env::temp_dir().join(format!("zcode-gpui-ta-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let foreign = dir.join("important-user-file.txt");
        std::fs::write(&foreign, b"keep me").unwrap();
        delete_owned(&foreign);
        assert!(foreign.exists(), "never delete files we do not own");
        std::fs::remove_dir_all(&dir).ok();
    }
}
