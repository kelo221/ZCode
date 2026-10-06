use super::*;
use crate::backend::workspace::{Pending, WorkspaceHandle};
use gpui::{AppContext, TestAppContext};
use serde_json::json;

#[gpui::test]
fn inspection_responses_and_plugin_refresh_stay_with_origin(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let a = s.workspaces[0].key.clone();
        let mut b = WorkspaceHandle::new(std::env::temp_dir().join("inspection-b"), vec![]);
        let key = b.key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        b.inbound = Some(tx);
        b.started = true;
        b.pending.insert(1, Pending::FetchMcpList);
        b.pending.insert(2, Pending::FetchPluginsOverview);
        b.pending.insert(3, Pending::PluginAction("plugins/marketplace/remove".into()));
        s.workspaces.push(b);
        s.active_workspace = Some(a);
        s.handle_response(&key, 1, Some(json!({"statuses":{"b-server":{"status":"connected","transport":"http","toolCount":0,"updatedAt":"2026-10-06T10:00:00Z"}}})), None, cx);
        s.handle_response(&key, 2, Some(json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})), None, cx);
        assert!(s.active_inspection().unwrap().mcp.value.is_none());
        assert_eq!(s.ws(&key).unwrap().inspection.mcp.value.as_ref().unwrap()[0].name, "b-server");
        s.handle_response(&key, 3, Some(json!({"diagnostics":[]})), None, cx);
        let request: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "plugins/overview");
        assert_eq!(request["params"]["workspace"]["workspacePath"], s.ws(&key).unwrap().path.to_string_lossy().as_ref());
        s.ws_mut(&key).unwrap().invalidate_connection();
        assert!(s.ws(&key).unwrap().inspection.mcp.value.is_none());
    });
}

#[gpui::test]
fn inspection_suppresses_duplicates_and_errors_do_not_poison_other_scope(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.fetch_mcp_servers(cx);
        s.fetch_mcp_servers(cx);
        let request: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert!(rx.try_recv().is_err());
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"token=sentinel-secret"})),
            cx,
        );
        let query = &s.workspaces[0].inspection.mcp;
        assert!(!query.loading);
        assert!(query.attempted);
        assert!(!query.error.as_ref().unwrap().contains("sentinel-secret"));
        s.fetch_usage_stats("7d", cx);
        s.fetch_usage_stats("30d", cx);
        assert_eq!(s.workspaces[0].inspection.usage_range, "30d");
        let first: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(
            &key,
            first["id"].as_u64().unwrap(),
            Some(json!({})),
            None,
            cx,
        );
        assert_eq!(s.workspaces[0].inspection.usage_range, "30d");
        let late: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.workspaces[0].invalidate_connection();
        s.handle_response(
            &key,
            late["id"].as_u64().unwrap(),
            Some(json!({})),
            None,
            cx,
        );
        assert!(s.workspaces[0].inspection.usage.is_empty());
    });
}
