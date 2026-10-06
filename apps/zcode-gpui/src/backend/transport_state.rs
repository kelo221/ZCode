//! Transport-state ownership and lifecycle for AppState (follow-up review
//! finding 2): the subscription registry is the authoritative topic owner,
//! and one reset operation serves reconnect, route replacement, idle unload,
//! and overflow restarts. Split from app/store.rs for the 400-line cap.

use crate::app::store::AppState;
use gpui::Context;

impl AppState {
    /// Latch the per-workspace flow signal; sends only on state change.
    pub(crate) fn flow_latch(&mut self, ws_key: &str, saturated: bool) {
        let current = self.flow_saturated.get(ws_key).copied().unwrap_or(false);
        if current == saturated {
            return;
        }
        self.flow_saturated.insert(ws_key.to_string(), saturated);
        self.send_flow_flag(ws_key, saturated);
        self.push_log(format!(
            "backpressure: {} streaming for {ws_key}",
            if saturated { "paused" } else { "resumed" }
        ));
    }

    /// Workspace key that owns a topic (for targeted resyncs). The live
    /// subscription registry is authoritative — a newly created conversation
    /// is subscribed before it appears in the sessions index, so deriving
    /// ownership from session rows alone left fresh topics unowned.
    pub(crate) fn ws_for_topic(&self, topic: &str) -> Option<String> {
        for w in &self.workspaces {
            if w.subscriptions.contains_key(topic) {
                return Some(w.key.clone());
            }
        }
        if let Some(k) = topic
            .strip_prefix("sessions-index/")
            .or_else(|| topic.strip_prefix("workspace-config/"))
        {
            return Some(k.to_string());
        }
        if let Some(ws_key) =
            crate::conversation::subagent_nav::child_topic_workspace(&self.child_owner, topic)
        {
            return Some(ws_key);
        }
        topic.strip_prefix("conversation/").and_then(|sid| {
            self.workspaces
                .iter()
                .find(|w| w.sessions.iter().any(|s| s.session_id == sid))
                .map(|w| w.key.clone())
        })
    }

    /// The single transport-state reset for a workspace: collects the exact
    /// topic keys from the subscription registry BEFORE clearing it, then
    /// drops cursors, fragment state, tombstones and subscribed flags for
    /// every live route — including child and non-indexed conversation
    /// topics (review finding 2).
    pub(crate) fn clear_workspace_transport_state(&mut self, ws_key: &str) {
        let topics: Vec<String> = self
            .ws(ws_key)
            .map(|w| w.subscriptions.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.subscriptions.clear();
            ws.desired_conversation = None;
        }
        for topic in &topics {
            self.route_cursors.remove(topic);
        }
        self.assembler.forget_topics(&topics);
        for topic in &topics {
            if let Some(sid) = topic.strip_prefix("conversation/")
                && let Some(c) = self.conversations.get_mut(sid)
            {
                c.subscribed = false;
            }
        }
    }

    /// Tear the workspace's agent down completely (idle unload): kill the
    /// child, then run the shared transport-state reset so no subscription,
    /// cursor, fragment group or flow latch outlives the connection.
    pub(crate) fn unload_workspace(&mut self, key: &str) {
        if let Some(ws) = self.ws_mut(key) {
            ws.unload();
        }
        self.clear_workspace_transport_state(key);
        self.flow_saturated.remove(key);
    }

    /// Controlled full connection restart (protocol backlog overflow):
    /// dropped stdout lines leave holes in the protocol stream that route
    /// resync cannot repair (lost ACKs, startup events, reverse RPC), so the
    /// whole connection is rebuilt through the normal exit path.
    pub(crate) fn restart_connection(&mut self, ws_key: &str, cx: &mut Context<Self>) {
        self.push_log(format!(
            "protocol backlog overflow on {ws_key}: restarting connection"
        ));
        self.reset_connection_state(ws_key);
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.shutdown();
        }
        // 旧 EOF 已被 generation 隔离，溢出重启必须显式启动新连接。
        self.spawn_workspace(ws_key, cx);
        cx.notify();
    }
}
