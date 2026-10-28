use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use gpui::TestApp;
use serde_json::{Value, json};

fn entry(sid: &str, phase: &str, at: i64) -> Value {
    json!({ "sessionId": sid, "title": sid, "phase": phase, "lastActivityAt": at })
}

fn upsert(state: &mut AppState, ws_key: &str, session: Value) {
    state
        .apply_sessions_index(
            ws_key,
            "deltas",
            &json!({ "deltas": [{ "op": "session.upserted", "session": session }] }),
        )
        .unwrap();
}

fn sort_stamp(state: &AppState, ws_key: &str, sid: &str) -> Option<i64> {
    state
        .ws(ws_key)
        .and_then(|ws| ws.sessions.iter().find(|s| s.session_id == sid))
        .and_then(|s| s.sort_activity_at)
}

#[test]
fn running_sessions_keep_their_sort_stamp_until_the_run_ends() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.apply_sessions_index(
            &key,
            "snapshot",
            &json!({ "snapshot": { "sessions": [entry("a", "completedSuccess", 100)] } }),
        )
        .unwrap();

        // The user's message starts a run: the stamp moves to the send time.
        upsert(s, &key, entry("a", "running", 200));
        assert_eq!(sort_stamp(s, &key, "a"), Some(200));
        // Every agent step bumps lastActivityAt; the sort stamp holds still.
        upsert(s, &key, entry("a", "running", 250));
        upsert(s, &key, entry("a", "running", 300));
        assert_eq!(sort_stamp(s, &key, "a"), Some(200));
        // A snapshot mid-run (reconnect) keeps it too.
        s.apply_sessions_index(
            &key,
            "snapshot",
            &json!({ "snapshot": { "sessions": [entry("a", "running", 350)] } }),
        )
        .unwrap();
        assert_eq!(sort_stamp(s, &key, "a"), Some(200));
        // The run finishes: the stamp catches up once.
        upsert(s, &key, entry("a", "completedSuccess", 400));
        assert_eq!(sort_stamp(s, &key, "a"), Some(400));
        // Seen for the first time while already running: its current time.
        upsert(s, &key, entry("b", "running", 500));
        assert_eq!(sort_stamp(s, &key, "b"), Some(500));
        upsert(s, &key, entry("b", "running", 600));
        assert_eq!(sort_stamp(s, &key, "b"), Some(500));
    });
}
