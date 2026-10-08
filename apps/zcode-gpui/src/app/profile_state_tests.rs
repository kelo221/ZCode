use crate::app::profile_state::{ProfileMutation, ProfileRequest};
use crate::app::store::AppState;
use crate::backend::services_rpc::ServiceValue;
use gpui::TestApp;
use serde_json::json;

fn inventory() -> serde_json::Value {
    json!({"agents":[],"userAgents":[],"pluginAgents":[],"capability":{"userScopeAvailable":true}})
}

#[test]
fn stale_inventory_cannot_overwrite_newer_scope_query() {
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        let q = s.profiles.queries.entry("user".into()).or_default();
        q.serial = 2;
        q.loading = true;
        s.settle_profile_request(
            ProfileRequest::List {
                scope: "user".into(),
                serial: 1,
            },
            Ok(ServiceValue::Json(inventory())),
            cx,
        );
        assert!(s.profiles.queries["user"].snapshot.is_none());
        assert!(s.profiles.queries["user"].loading);
        s.settle_profile_request(
            ProfileRequest::List {
                scope: "other".into(),
                serial: 2,
            },
            Ok(ServiceValue::Json(inventory())),
            cx,
        );
        assert!(s.profiles.queries["user"].snapshot.is_none());
    });
}

#[test]
fn void_commit_is_success_not_undefined_read_failure() {
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        let query = s.profiles.queries.entry("user".into()).or_default();
        query.serial = 1;
        query.loading = true;
        s.profiles.mutation_pending = true;
        let mutation = ProfileMutation {
            origin: Default::default(),
            receipt: "saved".into(),
            scope: "user".into(),
            method: "setEnabled".into(),
            params: json!({}),
            baseline: None,
        };
        s.settle_profile_request(
            ProfileRequest::Commit(mutation),
            Ok(ServiceValue::Undefined),
            cx,
        );
        assert!(!s.profiles.mutation_pending);
        assert!(s.profiles.outcomes["saved"].is_ok());
        // 提交会推进查询序号；旧回复应被忽略，刷新失败断言必须使用提交后的序号。
        let refresh_serial = s.profiles.queries["user"].serial;
        assert_eq!(refresh_serial, 2);
        s.settle_profile_request(
            ProfileRequest::List {
                scope: "user".into(),
                serial: refresh_serial,
            },
            Err("refresh failed".into()),
            cx,
        );
        assert!(s.profiles.outcomes["saved"].is_ok());
        assert_eq!(
            s.profiles.queries["user"].error.as_deref(),
            Some("refresh failed")
        );
    });
}

#[test]
fn interrupted_commit_blocks_replay_and_preserves_truthful_outcome() {
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        let mutation = ProfileMutation {
            origin: Default::default(),
            receipt: "unknown".into(),
            scope: "user".into(),
            method: "createAgent".into(),
            params: json!({}),
            baseline: None,
        };
        s.settle_profile_request(
            ProfileRequest::Commit(mutation),
            Err("Connection closed".into()),
            cx,
        );
        assert!(s.profiles.uncertain);
        assert!(s.profiles.outcomes["unknown"].is_err());
        assert!(s.profiles.queue.is_empty());
    });
}

#[test]
fn bounded_receipts_keep_latest_committed_result() {
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |state, cx| {
        for index in 0..70 {
            let mutation = ProfileMutation {
                origin: Default::default(),
                receipt: index.to_string(),
                scope: "user".into(),
                method: "setEnabled".into(),
                params: json!({}),
                baseline: None,
            };
            state.settle_profile_request(
                ProfileRequest::Commit(mutation),
                Ok(ServiceValue::Undefined),
                cx,
            );
        }
        assert_eq!(state.profiles.outcomes.len(), 64);
        assert!(state.profiles.outcomes["69"].is_ok());
        assert!(!state.profiles.outcomes.contains_key("0"));
    });
}

#[test]
fn production_profile_navigation_cannot_start_host_or_accept_mutations() {
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        s.read_profiles("user", true, cx);
        s.submit_profile_mutation(
            "blocked".into(),
            "user".into(),
            "createAgent",
            json!({}),
            None,
            cx,
        );
        assert!(!s.profiles.starting);
        assert!(!s.profiles.mutation_pending);
        assert!(s.profiles.client.is_none());
        assert!(s.profiles.queue.is_empty());
    });
}
