//! Payload builders for plugin store protocol methods:
//! `plugins/{overview,install,uninstall,update,setEnabled,restoreBuiltin,marketplace/*}`.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts and CONTEXT.md.

use serde_json::{Value, json};
use std::path::Path;

/// `zcodeWorkspaceRefSchema` is strict and requires `workspaceKey`; without
/// it every `plugins/*` and `mcp/list` request fails validation (-32602).
/// `workspace` is the canonical workspace key (plain path, no `\\?\` prefix),
/// the same identity the session commands use.
pub fn workspace_ref(workspace: &Path) -> Value {
    let key = workspace.to_string_lossy();
    json!({ "workspacePath": key, "workspaceKey": key })
}

pub fn plugins_overview_payload(workspace_path: &Path) -> Value {
    json!({
        "workspace": workspace_ref(workspace_path)
    })
}

pub fn install_plugin_payload(
    workspace_path: &Path,
    plugin_name: &str,
    marketplace: &str,
    scope: Option<&str>,
    dry_run: Option<bool>,
    operation_id: Option<&str>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path),
        "pluginName": plugin_name,
        "marketplace": marketplace,
    });
    if let Some(scope) = scope {
        payload["scope"] = json!(scope);
    }
    if let Some(dry_run) = dry_run {
        payload["dryRun"] = json!(dry_run);
    }
    if let Some(op_id) = operation_id {
        payload["operationId"] = json!(op_id);
    }
    payload
}

pub fn uninstall_plugin_payload(
    workspace_path: &Path,
    plugin_id: Option<&str>,
    plugin_name: Option<&str>,
    marketplace: Option<&str>,
    remove_cache: Option<bool>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path)
    });
    if let Some(id) = plugin_id {
        payload["pluginId"] = json!(id);
    }
    if let Some(name) = plugin_name {
        payload["pluginName"] = json!(name);
    }
    if let Some(mp) = marketplace {
        payload["marketplace"] = json!(mp);
    }
    if let Some(rc) = remove_cache {
        payload["removeCache"] = json!(rc);
    }
    payload
}

pub fn update_plugin_payload(
    workspace_path: &Path,
    plugin_id: Option<&str>,
    marketplace: Option<&str>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path)
    });
    if let Some(id) = plugin_id {
        payload["pluginId"] = json!(id);
    }
    if let Some(mp) = marketplace {
        payload["marketplace"] = json!(mp);
    }
    payload
}

pub fn set_plugin_enabled_payload(
    workspace_path: &Path,
    plugin_id: &str,
    enabled: bool,
    scope: Option<&str>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path),
        "pluginId": plugin_id,
        "enabled": enabled,
    });
    if let Some(scope) = scope {
        payload["scope"] = json!(scope);
    }
    payload
}

pub fn restore_builtin_plugin_payload(workspace_path: &Path, plugin_id: &str) -> Value {
    json!({
        "workspace": workspace_ref(workspace_path),
        "pluginId": plugin_id,
    })
}

#[allow(dead_code)]
pub fn add_marketplace_payload(
    workspace_path: &Path,
    source: &str,
    dry_run: Option<bool>,
    operation_id: Option<&str>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path),
        "source": source,
    });
    if let Some(dr) = dry_run {
        payload["dryRun"] = json!(dr);
    }
    if let Some(op) = operation_id {
        payload["operationId"] = json!(op);
    }
    payload
}

pub fn remove_marketplace_payload(workspace_path: &Path, marketplace: &str) -> Value {
    json!({
        "workspace": workspace_ref(workspace_path),
        "marketplace": marketplace,
    })
}

#[allow(dead_code)]
pub fn update_marketplace_payload(
    workspace_path: &Path,
    marketplace: Option<&str>,
    operation_id: Option<&str>,
) -> Value {
    let mut payload = json!({
        "workspace": workspace_ref(workspace_path)
    });
    if let Some(mp) = marketplace {
        payload["marketplace"] = json!(mp);
    }
    if let Some(op) = operation_id {
        payload["operationId"] = json!(op);
    }
    payload
}
