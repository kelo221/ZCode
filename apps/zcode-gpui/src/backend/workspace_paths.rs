use super::workspace::canonical_workspace_string;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The dedicated directory used for chats without a project (Tasks section).
/// Desktop parity: packages/services/src/paths.ts `getConversationWorkspaceDir()`
/// (`~/.zcode/workspace/default`).
pub fn conversation_workspace_dir() -> PathBuf {
    crate::shared::data_paths::paths()
        .data_root()
        .join("workspace")
        .join("default")
}

/// Ensures the default conversation workspace directory exists on disk.
pub fn ensure_conversation_workspace_dir() -> PathBuf {
    let dir = conversation_workspace_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The user's known projects: the desktop persists them in
/// `~/.zcode/v2/setting.json` (`recentProjects` + `lastWorkspaceSession`,
/// see packages/ui/src/hooks/useTabPersistence.ts). There is no RPC for this,
/// so we read the same file. Remote workspaces (ssh/wsl/docker) are skipped —
/// they need connection infrastructure the minimal client doesn't have.
pub fn discover_workspaces(primary: &Path, max: usize) -> Vec<PathBuf> {
    let conv_dir = conversation_workspace_dir();
    let conv_key = canonical_workspace_string(&conv_dir);
    let primary_key = canonical_workspace_string(primary);

    let mut ordered: Vec<PathBuf> = if primary_key == conv_key {
        Vec::new()
    } else {
        vec![primary.to_path_buf()]
    };

    let Ok(raw) = std::fs::read_to_string(settings_path()) else {
        return ordered;
    };
    let Ok(v) = serde_json::from_str::<Value>(&raw) else {
        return ordered;
    };
    let mut push = |p: &str| {
        let path = PathBuf::from(p);
        if path.is_dir() {
            let key = canonical_workspace_string(&path);
            if key != conv_key && !ordered.iter().any(|w| canonical_workspace_string(w) == key) {
                ordered.push(path);
            }
        }
    };
    if let Some(entries) = v.get("lastWorkspaceSession").and_then(Value::as_array) {
        for e in entries {
            if e.get("kind").and_then(Value::as_str) != Some("local") {
                continue;
            }
            if e.get("workspacePurpose").and_then(Value::as_str) == Some("conversation") {
                continue;
            }
            if let Some(p) = e.get("workspacePath").and_then(Value::as_str) {
                push(p);
            }
        }
    }
    if let Some(recents) = v.get("recentProjects").and_then(Value::as_array) {
        for r in recents {
            if let Some(p) = r.as_str() {
                push(p);
            }
        }
    }
    ordered.truncate(max);
    ordered
}

fn settings_path() -> PathBuf {
    crate::shared::data_paths::paths().settings_file()
}
