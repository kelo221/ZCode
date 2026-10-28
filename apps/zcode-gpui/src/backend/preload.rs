//! Conversation preloading: the first time a workspace's session list arrives
//! in this run (app start for the startup project, first open for the others),
//! its latest sessions are subscribed in the background, so opening one shows
//! the transcript at once instead of waiting for a snapshot.

use crate::app::store::AppState;
use crate::backend::pending::Pending;

/// Latest sessions per workspace to preload; matches the sidebar's first page.
pub(crate) const PRELOAD_COUNT: usize = 3;

impl AppState {
    /// `was_empty`: the workspace had no session list before this snapshot.
    pub(crate) fn preload_recent_conversations(&mut self, ws_key: &str, was_empty: bool) {
        if !was_empty {
            return;
        }
        let Some(ws) = self.ws(ws_key) else { return };
        let in_flight = |sid: &str| {
            ws.pending
                .values()
                .any(|p| matches!(p, Pending::SubscribeConversation(s) if s == sid))
        };
        let targets: Vec<String> = ws
            .sessions
            .iter()
            .take(PRELOAD_COUNT)
            .map(|s| s.session_id.clone())
            .filter(|sid| {
                !in_flight(sid) && self.conversations.get(sid).is_none_or(|c| !c.subscribed)
            })
            .collect();
        for sid in targets {
            self.subscribe_conversation(ws_key, &sid);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
    use crate::conversation::model::{ConversationState, SessionEntry};
    use gpui::TestApp;
    use serde_json::Value;

    fn entry(sid: &str) -> SessionEntry {
        SessionEntry {
            session_id: sid.into(),
            title: sid.into(),
            phase: "completedSuccess".into(),
            last_activity_at: None,
            sort_activity_at: None,
            preview: String::new(),
            has_background_work: false,
        }
    }

    #[test]
    fn first_session_list_subscribes_the_latest_three_once() {
        let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut app = TestApp::new();
        let state = app.new_entity(AppState::for_test);
        let (tx, rx) = std::sync::mpsc::channel();
        app.update_entity(&state, |s, _| {
            let ws = &mut s.workspaces[0];
            ws.inbound = Some(tx);
            ws.started = true;
            ws.sessions = ["a", "b", "c", "d"].map(entry).to_vec();
            let open = ConversationState {
                subscribed: true,
                ..Default::default()
            };
            s.conversations.insert("b".into(), open);
            let key = s.workspaces[0].key.clone();
            s.preload_recent_conversations(&key, true);
            // A later snapshot (reconnect, respawn) doesn't preload again.
            s.preload_recent_conversations(&key, false);
            s.preload_recent_conversations(&key, true);
        });
        let topics: Vec<String> = rx
            .try_iter()
            .map(|l| serde_json::from_str::<Value>(&l).unwrap())
            .filter(|v| v["method"] == "v4/conversation/subscribe")
            .map(|v| v["params"]["topic"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(topics, ["conversation/a", "conversation/c"]);
    }
}
