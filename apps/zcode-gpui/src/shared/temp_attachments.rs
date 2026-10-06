//! Lifecycle owner for temp files created from pasted clipboard images.
//!
//! The composer used to write timestamp-named files into the global temp dir
//! with no ownership tracking, so every paste leaked a file (2026-10-05 audit
//! P1.7). Each paste now uses a unique file under a `zcode-gpui-paste-`
//! directory and is deleted through explicit producer ownership: on remove, on app
//! quit (retired after submit), and via a stale sweep at startup.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Prefix identifying temp files this app owns (the sweep only ever deletes
/// files carrying it).
pub(crate) const TEMP_PREFIX: &str = "zcode-gpui-paste-";

/// Private per-process paste directory: another GPUI process can never
/// own, reuse or delete this process's paste files (review finding 10).
fn paste_dir() -> PathBuf {
    std::env::temp_dir().join(format!("{TEMP_PREFIX}{}", std::process::id()))
}

/// Write a separate file per paste; the caller owns deletion through [`delete_owned`].
pub(crate) fn write_temp_image(bytes: &[u8], ext: &str) -> std::io::Result<PathBuf> {
    let dir = paste_dir();
    std::fs::create_dir_all(&dir)?;
    // 同内容的第二次粘贴不能复用已提交路径，否则移除新附件会删除 runtime 仍在读取的文件。
    let path = dir.join(format!(
        "{TEMP_PREFIX}{}.{}",
        uuid::Uuid::now_v7(),
        ext.trim_start_matches('.')
    ));
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

/// Startup sweep: remove paste files older than 24h (crash leftovers),
/// including whole directories left behind by dead processes. Only paths
/// carrying the owned prefix are ever considered.
pub(crate) fn scavenge_stale() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let max_age = Duration::from_secs(24 * 3600);
    let stale = |path: &Path| {
        path.metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age >= max_age)
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(TEMP_PREFIX) {
            continue;
        }
        if entry.path().is_dir() {
            if let Ok(files) = std::fs::read_dir(entry.path()) {
                for file in files.flatten() {
                    if stale(&file.path()) {
                        let _ = std::fs::remove_file(file.path());
                    }
                }
            }
            // Only succeeds when the directory is empty.
            let _ = std::fs::remove_dir(entry.path());
        } else if stale(&entry.path()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_pastes_have_independent_lifetimes() {
        let a = write_temp_image(b"hello image", "png").unwrap();
        let b = write_temp_image(b"hello image", "png").unwrap();
        assert_ne!(a, b, "a later paste must not own an already submitted path");
        delete_owned(&b);
        assert!(a.exists());
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
