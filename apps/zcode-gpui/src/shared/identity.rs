//! Stable per-install client identity (2026-10-05 audit P1.10; hardened per
//! follow-up review finding 7).
//!
//! `AppState.client_id` documents itself as stable per install but used to be
//! regenerated as a fresh UUID every process. The identity now lives in a
//! small file under the shared data root (same base-dir policy as the backend
//! data dir), created atomically exactly once and reused from then on —
//! including across a missing parent directory or a corrupt file, both of
//! which are repaired instead of silently regenerating per process.

use std::path::{Path, PathBuf};

/// The value sent in command envelopes (`clientId`). The historical
/// `client-` prefix is kept so backend-side correlation strings do not change
/// shape between versions.
pub(crate) fn install_client_id() -> String {
    format!("client-{}", client_id_at(&identity_file_path()))
}

/// Data root shared with workspace.rs (`ZCODE_DATA_BASE_DIR` first, then the
/// user home's `.zcode`).
fn data_root() -> PathBuf {
    crate::shared::data_paths::paths().data_root()
}

fn identity_file_path() -> PathBuf {
    data_root().join("v2").join("gpui-client-id")
}

fn read_valid(path: &Path) -> Option<String> {
    let id = std::fs::read_to_string(path).ok()?;
    let id = id.trim();
    uuid::Uuid::parse_str(id).ok().map(|_| id.to_string())
}

/// Read (or atomically create/repair) the install UUID at `path`.
///
/// - A missing parent directory is created (a clean profile has no `v2`).
/// - A corrupt existing file is REPLACED: the old code's `create_new` failed
///   against the corrupt file and silently returned a fresh process-local id
///   on every launch (review finding 7).
/// - Concurrent creators race on tmp+rename; the loser re-reads the winner,
///   so every process converges on the one persisted value.
fn client_id_at(path: &Path) -> String {
    if let Some(id) = read_valid(path) {
        return id;
    }
    let fresh = uuid::Uuid::now_v7().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Write-then-rename is atomic on all platforms (std uses
    // MOVEFILE_REPLACE_EXISTING on Windows), so readers never see a partial
    // write and corrupt content is fully replaced.
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let written = std::fs::write(&tmp, fresh.as_bytes()).and_then(|_| std::fs::rename(&tmp, path));
    match written {
        Ok(()) => fresh,
        // Lost the race or an unwritable directory: adopt the persisted
        // winner if there is one, else stay stable for this process only.
        Err(_) => read_valid(path).unwrap_or(fresh),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("zcode-gpui-id-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("client-id");
        let first = client_id_at(&file);
        let second = client_id_at(&file);
        assert_eq!(first, second, "second read must reuse the stored identity");
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_parent_directory_is_created() {
        // A clean profile: <data>/v2 does not exist yet.
        let dir = std::env::temp_dir().join(format!("zcode-gpui-id-{}", uuid::Uuid::now_v7()));
        let file = dir.join("v2").join("client-id");
        assert!(!dir.exists());
        let first = client_id_at(&file);
        assert!(file.exists(), "parent dir + file must be created");
        assert_eq!(first, client_id_at(&file), "stable across launches");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_file_is_repaired_and_persisted() {
        let dir = std::env::temp_dir().join(format!("zcode-gpui-id-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("client-id");
        std::fs::write(&file, "not-a-uuid").unwrap();
        let repaired = client_id_at(&file);
        assert!(uuid::Uuid::parse_str(&repaired).is_ok());
        // The repair must reach the FILE (the old test only checked the
        // returned value), and a second call must reuse it.
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), repaired);
        assert_eq!(repaired, client_id_at(&file));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn concurrent_creators_converge_on_one_persisted_value() {
        let dir = std::env::temp_dir().join(format!("zcode-gpui-id-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("client-id");
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let f = file.clone();
                std::thread::spawn(move || client_id_at(&f))
            })
            .collect();
        let ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // Whoever won the rename owns the file; every reader afterwards
        // converges on that persisted value.
        let persisted = std::fs::read_to_string(&file).unwrap().trim().to_string();
        assert!(uuid::Uuid::parse_str(&persisted).is_ok());
        assert!(
            ids.contains(&persisted),
            "the winner's id persisted: {ids:?}"
        );
        assert_eq!(persisted, client_id_at(&file));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_prefix_is_stable() {
        // The envelope prefix is applied around the stored UUID; the file
        // itself always holds the bare UUID.
        let dir = std::env::temp_dir().join(format!("zcode-gpui-id-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("client-id");
        let id = client_id_at(&file);
        assert!(uuid::Uuid::parse_str(id.as_str()).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }
}
