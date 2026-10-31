//! Releases conversation subscriptions the user has left (PARITY.md,
//! "Conversation release"). A subscribed session can't be evicted from the
//! agent's residency pool, and every resident session owns its own stdio MCP
//! server processes, so threads that were opened once and never released
//! piled up to ~150 processes. Desktop drops a thread 30 s after it leaves the
//! screen; the maintenance sweep does the same here.

use crate::app::store::AppState;
use crate::conversation::model::ConversationState;
use serde_json::json;
use std::time::{Duration, Instant};

/// Matches Desktop's `SESSION_DATA_LAYER_KEEP_WARM_MS`.
pub(crate) const KEEP_WARM: Duration = Duration::from_secs(30);

impl AppState {
    /// One sweep: stamp newly releasable subscriptions, clear stamps that no
    /// longer apply, and release those releasable for at least `KEEP_WARM`.
    pub(crate) fn release_idle_conversations(&mut self, now: Instant) {
        let mut due = Vec::new();
        for ws in &self.workspaces {
            let phone_open = self.phone.has_open(&ws.key);
            for topic in ws.subscriptions.keys() {
                let Some(sid) = topic.strip_prefix("conversation/") else {
                    continue;
                };
                // Subagent children have their own release (`subagent_nav.rs`).
                if self.child_owner.contains_key(sid) {
                    continue;
                }
                let held = phone_open
                    || self.active.as_deref() == Some(sid)
                    || self.viewing_child.as_deref() == Some(sid);
                due.push((ws.key.clone(), sid.to_string(), held));
            }
        }
        let mut release = Vec::new();
        for (ws_key, sid, held) in due {
            let Some(c) = self.conversations.get_mut(&sid) else {
                continue;
            };
            if held || has_live_work(c) {
                c.releasable_since = None;
                continue;
            }
            let since = *c.releasable_since.get_or_insert(now);
            if now.duration_since(since) >= KEEP_WARM {
                release.push((ws_key, sid));
            }
        }
        for (ws_key, sid) in release {
            self.release_conversation(&ws_key, &sid);
        }
    }

    /// Unsubscribe one conversation topic and drop its route state. The
    /// mirror stays (with `subscribed = false`) so reopening shows the old rows
    /// until the fresh snapshot replaces them.
    fn release_conversation(&mut self, ws_key: &str, sid: &str) {
        let topic = format!("conversation/{sid}");
        let sub = self.ws_mut(ws_key).and_then(|ws| {
            let sub_id = ws.subscriptions.remove(&topic).map(|s| s.id)?;
            Some((sub_id, ws.connection_id.clone(), ws.next_id()))
        });
        if let Some((sub_id, conn_id, id)) = sub {
            self.send_request(
                ws_key,
                "v4/conversation/unsubscribe",
                json!({ "topic": topic, "subscriptionId": sub_id, "connectionId": conn_id }),
                id,
            );
        }
        self.route_cursors.remove(&topic);
        self.assembler.forget_topics(std::slice::from_ref(&topic));
        if let Some(c) = self.conversations.get_mut(sid) {
            c.subscribed = false;
            c.releasable_since = None;
        }
        self.push_log(format!("released idle conversation {sid}"));
    }
}

/// Work the user would lose sight of if the mirror stopped updating. An
/// auto-compaction in flight counts: it needs the subscription to finish.
pub(crate) fn has_live_work(c: &ConversationState) -> bool {
    c.phase_running()
        || c.auto_compact.is_some()
        || !c.pending_interactions.is_empty()
        || c.queue.as_ref().is_some_and(|q| !q.items.is_empty())
        || crate::app::activity::ActivityCounts::from_conversation(c).total() > 0
}

#[cfg(test)]
#[path = "conversation_release_tests.rs"]
mod tests;
