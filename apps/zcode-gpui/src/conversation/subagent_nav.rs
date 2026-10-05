//! Subagent child session navigation and read-only gating.
//!
//! Spec: Child conversation is an ordinary topic `conversation/<childSessionId>`.
//! The backend rejects input commands to child sessions (guard.subagentReadOnly).
//! The child view is strictly read-only: composer and turn actions are disabled.

use crate::app::store::AppState;
use gpui::Context;
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// Collect all child session IDs from known child owners and loaded subagent states.
pub fn collect_child_session_ids<'a>(
    child_owner: &'a HashMap<String, (String, String)>,
    subagent_states: impl IntoIterator<Item = &'a crate::conversation::subagents::SubagentsState>,
) -> HashSet<&'a str> {
    let mut ids = HashSet::new();
    for key in child_owner.keys() {
        ids.insert(key.as_str());
    }
    for sub in subagent_states {
        for id in &sub.child_session_ids {
            ids.insert(id.as_str());
        }
    }
    ids
}

/// Pure predicate checking if viewing child is set.
pub fn is_read_only(viewing_child: Option<&str>) -> bool {
    viewing_child.is_some()
}

/// Look up parent session id for the current child view.
pub fn parent_session_id<'a>(
    child_owner: &'a HashMap<String, (String, String)>,
    viewing_child: Option<&str>,
) -> Option<&'a str> {
    let child_sid = viewing_child?;
    child_owner.get(child_sid).map(|(_, p)| p.as_str())
}

/// Look up workspace for a child session topic.
pub fn child_topic_workspace(
    child_owner: &HashMap<String, (String, String)>,
    topic: &str,
) -> Option<String> {
    let child_sid = topic.strip_prefix("conversation/")?;
    child_owner.get(child_sid).map(|(ws, _)| ws.clone())
}

/// Determine active conversation id between child and parent.
pub fn active_conversation_sid<'a>(
    viewing_child: Option<&'a str>,
    active_parent: Option<&'a str>,
) -> Option<&'a str> {
    viewing_child.or(active_parent)
}

impl AppState {
    /// Whether the current view is a read-only subagent child view.
    pub fn is_read_only_view(&self) -> bool {
        is_read_only(self.viewing_child.as_deref())
    }

    /// Collect all known child subagent session IDs across child_owner and
    /// loaded conversations' subagents projections.
    pub fn all_child_session_ids(&self) -> HashSet<&str> {
        collect_child_session_ids(
            &self.child_owner,
            self.conversations
                .values()
                .filter_map(|c| c.subagents.as_ref()),
        )
    }

    /// Return the parent session id of the currently viewed subagent child, if any.
    pub fn parent_of_viewing_child(&self) -> Option<String> {
        parent_session_id(&self.child_owner, self.viewing_child.as_deref()).map(str::to_string)
    }

    /// Open a child subagent conversation in read-only mode.
    pub fn open_subagent(&mut self, child_sid: &str, cx: &mut Context<Self>) {
        let Some(parent_sid) = self.active.clone() else {
            return;
        };
        let Some(ws_key) = self.active_ws_key() else {
            return;
        };

        self.child_owner
            .insert(child_sid.to_string(), (ws_key.clone(), parent_sid));
        self.viewing_child = Some(child_sid.to_string());

        // Subscribe to the child conversation if not already subscribed.
        let is_subbed = self
            .conversations
            .get(child_sid)
            .is_some_and(|c| c.subscribed);
        if !is_subbed {
            self.subscribe_conversation(&ws_key, child_sid);
        }

        cx.notify();
    }

    /// Close the subagent view and return to the parent conversation.
    pub fn close_subagent(&mut self, cx: &mut Context<Self>) {
        self.clear_subagent_view();
        cx.notify();
    }

    /// Leave the child view (also on session or workspace switch) and release
    /// the child's subscription and mirror.
    pub fn clear_subagent_view(&mut self) {
        if let Some(child_sid) = self.viewing_child.take() {
            self.release_child(&child_sid);
        }
    }

    /// Unsubscribe a child conversation and drop every local trace of the
    /// subscription. Leaving `subscribed`/the subscription id behind would make
    /// a later `open_subagent` skip the resubscribe and show a frozen mirror;
    /// dropping the mirror keeps closed children from holding memory.
    fn release_child(&mut self, child_sid: &str) {
        let topic = format!("conversation/{child_sid}");
        let Some(ws_key) = self.child_owner.get(child_sid).map(|(ws, _)| ws.clone()) else {
            return;
        };
        let sub = self.ws_mut(&ws_key).and_then(|ws| {
            let sub_id = ws.subscriptions.remove(&topic).map(|s| s.id)?;
            Some((sub_id, ws.connection_id.clone(), ws.next_id()))
        });
        if let Some((sub_id, conn_id, id)) = sub {
            self.send_request(
                &ws_key,
                "v4/conversation/unsubscribe",
                json!({ "topic": topic, "subscriptionId": sub_id, "connectionId": conn_id }),
                id,
            );
        }
        self.route_cursors.remove(&topic);
        self.assembler.forget_topics(&[topic]);
        self.conversations.remove(child_sid);
    }
}

#[cfg(test)]
#[path = "subagent_nav_tests.rs"]
mod tests;
