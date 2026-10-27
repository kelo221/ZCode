//! Idle-agent maintenance for AppState (split from app/store.rs for the
//! 400-line cap): periodic sweep that unloads agents idle for too long and
//! resyncs routes whose frame assembly got stuck.

use crate::app::store::AppState;
use gpui::{AppContext, Context};

impl AppState {
    pub(crate) fn start_maintenance(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_spawn(async {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                })
                .await;
                let alive = this
                    .update(cx, |state, cx| {
                        state.reap_idle_agents();
                        state.maintain_profiles(cx);
                        state.flush_project_activity(cx);
                        // Resync routes whose frame assembly got stuck >30s.
                        for topic in state.assembler.sweep_timeouts() {
                            if let Some(ws_key) = state.ws_for_topic(&topic) {
                                state.resync_topic(&ws_key, &topic);
                            }
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    /// Kill agents with no open conversation that the user hasn't touched for
    /// a while. The active workspace and any workspace with a running turn are
    /// never unloaded.
    fn reap_idle_agents(&mut self) {
        let idle_secs: u64 = std::env::var("ZCODE_GPUI_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600);
        let active_ws = self.active_ws_key();
        let keys: Vec<String> = self.workspaces.iter().map(|w| w.key.clone()).collect();
        for key in keys {
            let Some(ws) = self.ws(&key) else { continue };
            let spawned = ws.started || ws.inbound.is_some();
            if !spawned {
                continue;
            }
            if Some(&key) == active_ws.as_ref() || self.phone.has_open(&key) {
                continue;
            }
            if !ws.pending.is_empty() || self.browser.has_pending(&key) {
                continue; // startup/handshake or a browser command in flight
            }
            if self.workspace_busy(&key)
                || ws
                    .last_activity
                    .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(idle_secs))
            {
                continue;
            }
            // unload_workspace runs the shared transport-state reset, so
            // subscriptions, cursors, fragment state and the flow latch die
            // with the connection (review finding 2).
            self.unload_workspace(&key);
            self.push_log(format!(
                "unloaded idle agent for {}",
                self.ws(&key).map(|w| w.display.clone()).unwrap_or(key)
            ));
        }
    }

    /// Whether unloading this workspace's agent would kill live work. The
    /// sessions-index fields cover sessions this client never opened (e.g.
    /// automations). Background subagents, shells and workflows exist only
    /// in the agent process's in-memory task registry: killing it ends them
    /// for good, and after a respawn TaskOutput/SendMessage can't find them.
    /// A waiting interaction pins the agent too, since unloading it would
    /// abort the blocked turn.
    pub(crate) fn workspace_busy(&self, key: &str) -> bool {
        let Some(ws) = self.ws(key) else {
            return false;
        };
        ws.sessions.iter().any(|s| {
            crate::conversation::model::phase_is_active(&s.phase)
                || s.has_background_work
                || self.conversations.get(&s.session_id).is_some_and(|c| {
                    c.phase_running()
                        || !c.pending_interactions.is_empty()
                        || crate::app::activity::ActivityCounts::from_conversation(c).total() > 0
                })
        })
    }
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;
