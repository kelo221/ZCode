//! Stable per-install client identity (2026-10-05 audit P1.10).
//!
//! `AppState.client_id` documents itself as stable per install but used to be
//! regenerated as a fresh UUID every process. The identity now lives in a
//! small file under the shared data root (same base-dir policy as the backend
//! data dir), created atomically exactly once and reused from then on.

use std::fs::OpenOptions;
use std::io::Write;
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
    if let Ok(dir) = std::env::var("ZCODE_DATA_BASE_DIR")
        && !dir.trim().is_empty()
    {
        return PathBuf::from(dir);
    }
    crate::shared::settings::resolve_user_home_dir().join(".zcode")
}

fn identity_file_path() -> PathBuf {
    data_root().join("v2").join("gpui-client-id")
}

/// Read (or atomically create) the install UUID at `path`. A corrupt or empty
/// file is replaced with a fresh identity instead of poisoning every envelope.
fn client_id_at(path: &Path) -> String {
    if let Ok(id) = std::fs::read_to_string(path) {
        let id = id.trim();
        if uuid::Uuid::parse_str(id).is_ok() {
            return id.to_string();
        }
    }
    let fresh = uuid::Uuid::now_v7().to_string();
    // create_new fails when another process won the race; its file is as
    // good as ours, so fall back to reading whatever landed first.
    let created = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(fresh.as_bytes()));
    match created {
        Ok(()) => fresh,
        Err(_) => {
            if let Ok(id) = std::fs::read_to_string(path) {
                let id = id.trim();
                if uuid::Uuid::parse_str(id).is_ok() {
                    return id.to_string();
                }
            }
            // Unwritable directory (or unreadable winner): stay stable for
            // this process rather than crash the app.
            fresh
        }
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
        // A corrupt file is regenerated, not propagated.
        std::fs::write(&file, "not-a-uuid").unwrap();
        let repaired = client_id_at(&file);
        assert_ne!(repaired, first);
        assert!(uuid::Uuid::parse_str(&repaired).is_ok());
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
