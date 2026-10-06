use super::*;
use gpui::{AppContext, TestAppContext};

fn page(ids: &[&str], cursor: Option<&str>) -> Value {
    let mut value = json!({"revision": 4, "childSessionIds": [], "running": [], "ended": {
        "total": 4, "items": ids.iter().map(|id| json!({"childSessionId":id,"subagentType":"Explore","title":id,"status":"success"})).collect::<Vec<_>>()
    }});
    if let Some(cursor) = cursor {
        value["ended"]["nextCursor"] = json!(cursor);
    }
    value
}

#[gpui::test]
fn directory_pages_are_scoped_and_do_not_replace_snapshot(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("a".into());
        s.conversations.entry("a".into()).or_default().subagents =
            Some(crate::conversation::subagents::SubagentsState {
                revision: 4,
                ended_total: 8,
                ..Default::default()
            });
        s.ensure_subagent_directory(false, false, cx);
        s.ensure_subagent_directory(false, false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "session/subagents");
        assert_eq!(request["params"], json!({"sessionId":"a","endedLimit":20}));
        assert!(rx.try_recv().is_err());
        s.active = Some("b".into());
        s.ensure_subagent_directory(false, false, cx);
        let second: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(second["params"]["sessionId"], "b");
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(page(&["child-1"], Some("next"))),
            None,
            cx,
        );
        assert_eq!(s.workspaces[0].subagent_directories["a"].items.len(), 1);
        assert!(s.workspaces[0].subagent_directories["b"].items.is_empty());
        assert_eq!(
            s.conversations["a"].subagents.as_ref().unwrap().ended_total,
            8
        );
        s.active = Some("a".into());
        s.ensure_subagent_directory(false, true, cx);
        let next: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(next["params"]["endedCursor"], "next");
        let mut value = page(&["child-1", "child-2"], None);
        value["ended"].as_object_mut().unwrap().remove("nextCursor");
        s.handle_response(&key, next["id"].as_u64().unwrap(), Some(value), None, cx);
        assert_eq!(s.workspaces[0].subagent_directories["a"].items.len(), 2);
        s.open_subagent_for(&key, "a", "child-2", cx);
        assert_eq!(s.viewing_child.as_deref(), Some("child-2"));
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].subagent_directories.is_empty());
    });
}

#[gpui::test]
fn directory_failure_retains_rows_and_refresh_is_explicit(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0]
            .subagent_directories
            .entry("a".into())
            .or_default()
            .loading = true;
        s.settle_subagent_directory(&key, "a", false, Err("token=sentinel-secret".into()), cx);
        let cache = &s.workspaces[0].subagent_directories["a"];
        assert!(!cache.loading);
        assert!(cache.error.as_ref().unwrap().contains("[REDACTED]"));
        assert!(!cache.error.as_ref().unwrap().contains("sentinel-secret"));
    });
}

#[test]
fn malformed_directory_is_not_membership_evidence() {
    let mut value = page(&["child"], None);
    value["ended"].as_object_mut().unwrap().remove("nextCursor");
    assert!(DirectoryResult::parse(value.clone()).is_ok());
    value["ended"]["items"][0]["status"] = json!("running");
    assert!(DirectoryResult::parse(value).is_err());
}
