use super::KEEP_WARM;
use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use crate::backend::workspace::RouteSubscription;
use crate::conversation::model::ConversationState;
use gpui::TestApp;
use serde_json::Value;
use std::time::{Duration, Instant};

/// A started workspace subscribed to `sids`, with `setup` applied, after two
/// sweeps `gap` apart. Returns the topics unsubscribed and the subscribed
/// flags afterwards.
fn sweep_twice(
    sids: &[&str],
    gap: Duration,
    setup: impl FnOnce(&mut AppState),
) -> (Vec<String>, Vec<(String, bool)>) {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    let flags = app.update_entity(&state, |s, _| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        for sid in sids {
            ws.subscriptions.insert(
                format!("conversation/{sid}"),
                RouteSubscription {
                    id: format!("sub-{sid}"),
                    log_epoch: "e".into(),
                },
            );
            let conv = ConversationState {
                subscribed: true,
                ..Default::default()
            };
            s.conversations.insert((*sid).into(), conv);
        }
        setup(s);
        let start = Instant::now();
        s.release_idle_conversations(start);
        s.release_idle_conversations(start + gap);
        let mut flags: Vec<(String, bool)> = sids
            .iter()
            .map(|sid| ((*sid).to_string(), s.conversations[*sid].subscribed))
            .collect();
        flags.sort();
        flags
    });
    let mut topics: Vec<String> = rx
        .try_iter()
        .map(|l| serde_json::from_str::<Value>(&l).unwrap())
        .filter(|v| v["method"] == "v4/conversation/unsubscribe")
        .map(|v| v["params"]["topic"].as_str().unwrap().to_string())
        .collect();
    topics.sort();
    (topics, flags)
}

#[test]
fn switching_away_releases_the_old_thread_after_the_keep_warm() {
    let (topics, flags) = sweep_twice(&["a", "b"], KEEP_WARM, |s| {
        s.active = Some("b".into());
    });
    assert_eq!(topics, ["conversation/a"]);
    assert_eq!(flags, [("a".into(), false), ("b".into(), true)]);
}

#[test]
fn nothing_is_released_inside_the_keep_warm() {
    let (topics, _) = sweep_twice(&["a"], KEEP_WARM - Duration::from_secs(1), |_| {});
    assert!(topics.is_empty());
}

#[test]
fn a_running_thread_is_not_released() {
    let (topics, _) = sweep_twice(&["a"], KEEP_WARM, |s| {
        s.conversations.get_mut("a").unwrap().phase = "running".into();
    });
    assert!(topics.is_empty());
}

#[test]
fn selecting_again_restarts_the_keep_warm() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, _| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        ws.subscriptions.insert(
            "conversation/a".into(),
            RouteSubscription {
                id: "sub-a".into(),
                log_epoch: "e".into(),
            },
        );
        let conv = ConversationState {
            subscribed: true,
            ..Default::default()
        };
        s.conversations.insert("a".into(), conv);
        let start = Instant::now();
        s.release_idle_conversations(start);
        // The user comes back to the thread, then leaves it again.
        s.active = Some("a".into());
        s.release_idle_conversations(start + KEEP_WARM / 2);
        s.active = None;
        s.release_idle_conversations(start + KEEP_WARM);
        assert!(s.conversations["a"].subscribed);
        s.release_idle_conversations(start + KEEP_WARM * 2);
        assert!(!s.conversations["a"].subscribed);
    });
    let unsubscribes = rx
        .try_iter()
        .filter(|l| l.contains("v4/conversation/unsubscribe"))
        .count();
    assert_eq!(unsubscribes, 1);
}
