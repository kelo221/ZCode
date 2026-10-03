use super::*;
use std::path::PathBuf;

#[test]
fn test_plugins_overview_payload() {
    let p = plugins_overview_payload(&PathBuf::from("/workspace/foo"));
    assert_eq!(p["workspace"]["workspacePath"], "/workspace/foo");
}

#[test]
fn test_install_plugin_payload() {
    let p = install_plugin_payload(
        &PathBuf::from("/ws"),
        "my-tool",
        "zcode-plugins-official",
        Some("workspace"),
        Some(false),
        Some("op-123"),
    );
    assert_eq!(p["pluginName"], "my-tool");
    assert_eq!(p["marketplace"], "zcode-plugins-official");
    assert_eq!(p["scope"], "workspace");
    assert_eq!(p["dryRun"], false);
    assert_eq!(p["operationId"], "op-123");
}

#[test]
fn test_uninstall_plugin_payload() {
    let p = uninstall_plugin_payload(
        &PathBuf::from("/ws"),
        Some("plugin-id-1"),
        Some("plugin-name"),
        Some("market-1"),
        Some(true),
    );
    assert_eq!(p["pluginId"], "plugin-id-1");
    assert_eq!(p["pluginName"], "plugin-name");
    assert_eq!(p["marketplace"], "market-1");
    assert_eq!(p["removeCache"], true);
}

#[test]
fn test_set_plugin_enabled_payload() {
    let p = set_plugin_enabled_payload(&PathBuf::from("/ws"), "plugin-1", false, Some("global"));
    assert_eq!(p["pluginId"], "plugin-1");
    assert_eq!(p["enabled"], false);
    assert_eq!(p["scope"], "global");
}

#[test]
fn test_restore_builtin_plugin_payload() {
    let p = restore_builtin_plugin_payload(&PathBuf::from("/ws"), "builtin-1");
    assert_eq!(p["pluginId"], "builtin-1");
}

#[test]
fn test_marketplace_payloads() {
    let add_p = add_marketplace_payload(
        &PathBuf::from("/ws"),
        "https://github.com/org/plugins",
        None,
        None,
    );
    assert_eq!(add_p["source"], "https://github.com/org/plugins");

    let rm_p = remove_marketplace_payload(&PathBuf::from("/ws"), "custom-market");
    assert_eq!(rm_p["marketplace"], "custom-market");

    let up_p = update_marketplace_payload(&PathBuf::from("/ws"), Some("custom-market"), None);
    assert_eq!(up_p["marketplace"], "custom-market");
}

/// `zcodeWorkspaceRefSchema` is strict: `workspacePath` and `workspaceKey`
/// are both required and nothing else is allowed. Missing `workspaceKey`
/// made every plugin-store request fail with -32602.
#[test]
fn every_payload_sends_a_strict_workspace_ref() {
    let ws = PathBuf::from(r"F:\proj");
    let payloads = [
        plugins_overview_payload(&ws),
        install_plugin_payload(&ws, "pdf", "zcode-plugins-official", None, None, None),
        uninstall_plugin_payload(&ws, Some("pdf"), None, None, None),
        update_plugin_payload(&ws, Some("pdf"), None),
        set_plugin_enabled_payload(&ws, "pdf", true, None),
        restore_builtin_plugin_payload(&ws, "pdf"),
        add_marketplace_payload(&ws, "https://example.invalid/m.git", None, None),
        remove_marketplace_payload(&ws, "mine"),
        update_marketplace_payload(&ws, None, None),
    ];
    for p in payloads {
        let w = p["workspace"].as_object().expect("workspace object");
        let mut keys: Vec<_> = w.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["workspaceKey", "workspacePath"], "{p}");
        assert_eq!(w["workspacePath"], r"F:\proj");
        assert_eq!(w["workspaceKey"], r"F:\proj");
    }
}
