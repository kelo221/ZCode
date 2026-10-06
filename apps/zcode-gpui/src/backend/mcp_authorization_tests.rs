use crate::app::store::AppState;
use crate::backend::workspace::WorkspaceHandle;
use crate::shared::mcp::{McpServerSnapshot, authorization_tests::fixture};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn mcp_authorization_receipts_reject_refresh_switch_replacement_and_reconnect(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.active_workspace = Some(key.clone());
        let ws = &mut s.workspaces[0];
        ws.started = true;
        ws.inbound = Some(tx);
        ws.inspection.mcp.finish(
            McpServerSnapshot::list_from_value(&fixture(Some(
                "https://auth.example.test/?state=one",
            )))
            .unwrap(),
        );
        let receipt = s.mcp_authorization_receipt("fixture").unwrap();
        assert_eq!(
            s.current_mcp_authorization_url(&receipt),
            Some("https://auth.example.test/?state=one")
        );
        let mut calls = 0;
        assert!(s.open_mcp_authorization(&receipt, |_| calls += 1));
        assert_eq!(calls, 1);
        s.active_workspace = None;
        assert!(!s.open_mcp_authorization(&receipt, |_| calls += 1));
        assert_eq!(calls, 1);
        assert!(s.current_mcp_authorization_url(&receipt).is_none());
        s.active_workspace = Some(key.clone());
        s.fetch_mcp_servers(cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "mcp/list");
        assert_eq!(request["params"]["mode"], "status");
        assert!(request["params"].get("mcpServers").is_none());
        assert!(s.current_mcp_authorization_url(&receipt).is_none());
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(fixture(Some("https://auth.example.test/?state=one"))),
            None,
            cx,
        );
        assert!(s.current_mcp_authorization_url(&receipt).is_none());
        let fresh = s.mcp_authorization_receipt("fixture").unwrap();
        s.workspaces[0].inspection.mcp.finish(
            McpServerSnapshot::list_from_value(&fixture(Some(
                "https://auth.example.test/?state=two",
            )))
            .unwrap(),
        );
        assert!(s.current_mcp_authorization_url(&fresh).is_none());
        let fresh = s.mcp_authorization_receipt("fixture").unwrap();
        s.active_workspace = None;
        s.mcp_authorization_open_failed(&fresh);
        assert!(s.workspaces[0].inspection.mcp.error.is_some());
        s.active_workspace = Some(key.clone());
        s.workspaces[0].inspection.mcp.error = None;
        s.workspaces[0].generation += 1;
        s.mcp_authorization_open_failed(&fresh);
        assert!(s.workspaces[0].inspection.mcp.error.is_none());
        assert!(s.current_mcp_authorization_url(&fresh).is_none());
        s.workspaces[0].invalidate_connection();
        assert!(s.mcp_authorization_receipt("fixture").is_none());
    });
}

#[gpui::test]
fn mcp_authorization_query_late_reply_is_origin_only_and_retired_after_invalidation(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let a = s.workspaces[0].key.clone();
        let mut b = WorkspaceHandle::new(std::env::temp_dir().join("mcp-auth-b"), vec![]);
        let key = b.key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        b.started = true;
        b.inbound = Some(tx);
        s.workspaces.push(b);
        s.active_workspace = Some(key.clone());
        s.fetch_mcp_servers(cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.active_workspace = Some(a.clone());
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(fixture(Some("https://auth.example.test/?state=one"))),
            None,
            cx,
        );
        assert!(s.ws(&a).unwrap().inspection.mcp.value.is_none());
        assert!(
            s.ws(&key).unwrap().inspection.mcp.value.as_ref().unwrap()[0]
                .authorization
                .is_some()
        );
        assert!(s.mcp_authorization_receipt("fixture").is_none());
        s.active_workspace = Some(key.clone());
        s.fetch_mcp_servers(cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.ws_mut(&key).unwrap().invalidate_connection();
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(fixture(Some("https://auth.example.test/?state=old"))),
            None,
            cx,
        );
        assert!(s.ws(&key).unwrap().inspection.mcp.value.is_none());
    });
}

#[gpui::test]
fn mcp_authorization_errors_cannot_store_unlabelled_url_tokens(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        s.active_workspace = Some(key.clone());
        s.fetch_mcp_servers(cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"unlabelled-secret https://auth.example.test/?state=secret"})),
            cx,
        );
        let ws = s.ws(&key).unwrap();
        assert!(!ws.status.contains("unlabelled-secret"));
        assert!(
            !ws.inspection
                .mcp
                .error
                .as_ref()
                .unwrap()
                .contains("unlabelled-secret")
        );
        assert!(
            !s.errors
                .iter()
                .any(|error| error.contains("unlabelled-secret"))
        );
        assert!(!s.log.iter().any(|line| line.contains("unlabelled-secret")));
        assert!(s.mcp_authorization_receipt("fixture").is_none());
    });
}
