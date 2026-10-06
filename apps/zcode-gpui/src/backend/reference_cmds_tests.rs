use crate::app::store::AppState;
use crate::composer::references::{CatalogKind, CatalogState};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn catalog_response_is_scoped_and_reset_clears_connection_cache(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.active_workspace = Some(key.clone());
        s.active = Some("session-a".into());
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.ensure_reference_catalog(CatalogKind::Skills, false, cx);
        s.ensure_reference_catalog(CatalogKind::Skills, false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["params"]["sessionId"], "session-a");
        assert_eq!(request["params"]["workspace"]["workspaceKey"], key);
        assert!(rx.try_recv().is_err());
        s.active = Some("session-b".into());
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(json!({"authority":"session","skills":[]})),
            None,
            cx,
        );
        assert!(matches!(
            s.workspaces[0].reference_catalogs[&Some("session-a".into())].skills,
            CatalogState::Ready(_)
        ));
        assert!(
            !s.workspaces[0]
                .reference_catalogs
                .contains_key(&Some("session-b".into()))
        );
        s.workspaces[0].invalidate_connection();
        assert!(s.workspaces[0].reference_catalogs.is_empty());
    });
}

#[gpui::test]
fn failed_session_lookup_never_retries_as_workspace(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.active_workspace = Some(key.clone());
        s.active = Some("unknown".into());
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.ensure_reference_catalog(CatalogKind::Plugins, false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            None,
            Some(json!({"message":"Session unavailable"})),
            cx,
        );
        s.ensure_reference_catalog(CatalogKind::Plugins, false, cx);
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            s.workspaces[0].reference_catalogs[&Some("unknown".into())].plugins,
            CatalogState::Failed(_)
        ));
        s.ensure_reference_catalog(CatalogKind::Plugins, true, cx);
        let retry: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(retry["params"]["sessionId"], "unknown");
    });
}
