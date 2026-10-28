use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use crate::conversation::model::{ConversationState, SessionEntry};
use crate::conversation::subagents::BackgroundWork;
use gpui::TestApp;

fn entry(sid: &str, has_background_work: bool) -> SessionEntry {
    SessionEntry {
        session_id: sid.into(),
        title: sid.into(),
        phase: "completedSuccess".into(),
        last_activity_at: None,
        sort_activity_at: None,
        preview: String::new(),
        has_background_work,
    }
}

/// An idle, started, non-active workspace (no user activity recorded), with
/// `setup` applied, after one reaper sweep. Returns whether its agent survived.
fn survives_sweep(setup: impl FnOnce(&mut AppState)) -> bool {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    let (tx, _rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, _| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        setup(s);
        s.reap_idle_agents();
        s.workspaces[0].started
    })
}

#[test]
fn idle_workspace_without_work_is_unloaded() {
    assert!(!survives_sweep(|s| {
        s.workspaces[0].sessions = vec![entry("parent", false)];
    }));
}

#[test]
fn index_background_work_keeps_the_agent_alive() {
    // Parent turn ended, three background subagents still running, and the
    // user switched to another project: the index is the only signal.
    assert!(survives_sweep(|s| {
        s.workspaces[0].sessions = vec![entry("parent", true)];
    }));
}

#[test]
fn open_conversation_background_work_keeps_the_agent_alive() {
    assert!(survives_sweep(|s| {
        s.workspaces[0].sessions = vec![entry("parent", false)];
        let conv = ConversationState {
            subscribed: true,
            background_works: vec![BackgroundWork {
                work_id: "w1".into(),
                kind: "bash".into(),
                status: "running".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        s.conversations.insert("parent".into(), conv);
    }));
}

#[test]
fn finished_background_work_does_not_pin_the_agent() {
    assert!(!survives_sweep(|s| {
        s.workspaces[0].sessions = vec![entry("parent", false)];
        let conv = ConversationState {
            subscribed: true,
            background_works: vec![BackgroundWork {
                work_id: "w1".into(),
                kind: "subagent".into(),
                status: "completed".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        s.conversations.insert("parent".into(), conv);
    }));
}

#[test]
fn sessions_index_parses_has_background_work() {
    let v = serde_json::json!({ "sessionId": "s", "hasBackgroundWork": true });
    assert!(SessionEntry::from_value(&v).unwrap().has_background_work);
    let v = serde_json::json!({ "sessionId": "s" });
    assert!(!SessionEntry::from_value(&v).unwrap().has_background_work);
}
