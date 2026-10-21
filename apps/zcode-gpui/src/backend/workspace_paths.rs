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
/// `~/.zcode/v2/setting.json` (`lastWorkspaceSession` is its projection of
/// every non-deleted local project root, `recentProjects` the most recent
/// ten; see packages/services/src/remote-projects/workspace-settings.ts).
/// There is no RPC for this, so we read the same file. Remote workspaces
/// (ssh/wsl/docker) are skipped — they need connection infrastructure the
/// minimal client doesn't have.
///
/// `explicit` is a folder named on the command line. The launch cwd is not a
/// project: a shortcut's working directory would otherwise show up as one.
pub fn discover_workspaces(explicit: Option<&Path>) -> Vec<PathBuf> {
    discover_workspaces_from(&settings_path(), explicit)
}

/// The list is uncapped: agents start lazily, so a long list costs nothing
/// until a project is opened, and a cap silently hid projects that still
/// exist (the desktop sidebar is unbounded too).
pub(crate) fn discover_workspaces_from(settings: &Path, explicit: Option<&Path>) -> Vec<PathBuf> {
    let conv_dir = conversation_workspace_dir();
    let conv_key = canonical_workspace_string(&conv_dir);

    let mut ordered: Vec<PathBuf> = Vec::new();
    if let Some(primary) = explicit
        && canonical_workspace_string(primary) != conv_key
    {
        ordered.push(primary.to_path_buf());
    }

    let Ok(raw) = std::fs::read_to_string(settings) else {
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
    // The last active project leads, so it is the one activated at startup.
    if let Some(p) = v.get("lastActiveWorkspace").and_then(local_project_path) {
        push(p);
    }
    if let Some(entries) = v.get("lastWorkspaceSession").and_then(Value::as_array) {
        for p in entries.iter().filter_map(local_project_path) {
            push(p);
        }
    }
    if let Some(recents) = v.get("recentProjects").and_then(Value::as_array) {
        for r in recents {
            if let Some(p) = r.as_str() {
                push(p);
            }
        }
    }
    ordered
}

/// A persisted workspace entry's path when it is a local project.
fn local_project_path(entry: &Value) -> Option<&str> {
    if entry.get("kind").and_then(Value::as_str) != Some("local") {
        return None;
    }
    if entry.get("workspacePurpose").and_then(Value::as_str) == Some("conversation") {
        return None;
    }
    entry.get("workspacePath").and_then(Value::as_str)
}

fn settings_path() -> PathBuf {
    crate::shared::data_paths::paths().settings_file()
}

#[cfg(test)]
#[path = "workspace_paths_tests.rs"]
mod tests;
