//! Response correlation: pending-entry resolution, subscription-id capture,
//! CAS config acks (stale retry), history pages.

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use gpui::Context;
use serde_json::Value;

impl AppState {
    pub(crate) fn handle_response(
        &mut self,
        ws_key: &str,
        id: u64,
        result: Option<Value>,
        error: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        if let Some(err) = error {
            // Background freshness probes fail silently: the next tick retries,
            // and a transient store hiccup must not banner the user.
            if let Some(ws) = self.ws_mut(ws_key)
                && matches!(ws.pending.get(&id), Some(Pending::PollRows(_)))
            {
                ws.pending.remove(&id);
                cx.notify();
                return;
            }
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("rpc error");
            self.status_error(msg);
            self.push_error(format!("request failed: {msg}"));
            let pending = self.ws_mut(ws_key).and_then(|ws| ws.pending.remove(&id));
            if let Some(Pending::Command(ctx)) = pending {
                self.rollback_command(ws_key, &ctx);
            }
            cx.notify();
            return;
        }
        // Take the pending entry (workspace borrow must end before we mutate
        // conversations below).
        let Some(pending) = self.ws_mut(ws_key).and_then(|ws| ws.pending.remove(&id)) else {
            return;
        };
        match pending {
            Pending::SubscribeIndex => {
                self.capture_subscription(ws_key, "sessions-index", &result);
                self.push_log("sessions-index subscribed".into());
            }
            Pending::SubscribeConfig => {
                self.capture_subscription(ws_key, "workspace-config", &result);
            }
            Pending::ModelCatalog => {
                // Harvest the model catalog from the legacy session/create
                // settings snapshot, then delete the throwaway session it
                // persisted (keeps the sidebar clean).
                let legacy_sid = result
                    .as_ref()
                    .and_then(|r| r.get("sessionId"))
                    .or_else(|| {
                        result
                            .as_ref()
                            .and_then(|r| r.pointer("/session/sessionId"))
                    })
                    .or_else(|| result.as_ref().and_then(|r| r.pointer("/record/sessionId")))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let ws_key = ws_key.to_string();
                if let Some(settings) = result.as_ref().and_then(|r| r.get("settings")) {
                    self.workspace_configs
                        .entry(ws_key.clone())
                        .or_default()
                        .apply_settings(settings);
                    self.push_log(format!(
                        "model catalog: {} models",
                        self.workspace_configs
                            .get(&ws_key)
                            .map(|c| c.models.len())
                            .unwrap_or(0)
                    ));
                }
                if let Some(sid) = legacy_sid {
                    self.delete_session(&ws_key, &sid);
                }
            }
            Pending::CleanupCatalog => {}
            Pending::SubscribeConversation(sid) => {
                let topic = format!("conversation/{sid}");
                self.capture_subscription(ws_key, &topic, &result);
                let c = self.conversations.entry(sid.clone()).or_default();
                c.subscribed = true;
                self.push_log(format!("subscribed conversation {sid}"));
            }
            Pending::CreateSession => {
                // RPC result = CommandAck { status, result: { type:
                // "createSession", sessionId, .. } } — the session id lives
                // one level below the ack.
                let sid = result
                    .as_ref()
                    .and_then(|ack| ack.get("result"))
                    .and_then(|r| r.get("sessionId"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(sid) = sid {
                    self.draft = false;
                    self.active = Some(sid.clone());
                    self.active_workspace = Some(ws_key.to_string());
                    self.ui_model_value = None;
                    self.ui_mode = None;
                    self.push_log("session created".into());
                    self.subscribe_conversation(ws_key, &sid);
                }
            }
            // The CAS revision is mirrored only from the conversation stream.
            // `revisionAtDecision + 1` was a guess, and was applied to whatever
            // session happened to be active when the ack arrived.
            Pending::SendText | Pending::Stop => {}
            Pending::Command(ctx) => {
                self.handle_command_ack(ws_key, result.as_ref(), ctx);
            }
            Pending::Resync => {
                self.push_log("resync acknowledged".into());
            }
            Pending::FetchRows(sid) => {
                let (rows, has_more) = match &result {
                    Some(r) => (
                        r.get("rows").and_then(Value::as_array).cloned(),
                        r.get("hasMore").and_then(Value::as_bool),
                    ),
                    None => (None, None),
                };
                let count = rows.as_ref().map(|r| r.len()).unwrap_or(0);
                if let Some(c) = self.conversations.get_mut(&sid) {
                    if let Some(rows) = rows {
                        c.apply_older_rows(&rows);
                    }
                    if let Some(true) = has_more {
                        // has_more false = we reached the beginning; the
                        // Load-earlier button derives from first_row_id, so
                        // nothing else to store here.
                    }
                    // `atRevision` describes when the page was read; it can be
                    // older than the live mirror, so it never overwrites it.
                }
                self.push_log(format!("history page: +{count} rows"));
            }
            Pending::PollRows(sid) => {
                // Change probe, never applied. The poll makes the backend
                // refresh its cold projection when the store was appended by
                // another process; movement here triggers the standard resync
                // whose snapshot repaints everything (rows, statuses, plan,
                // queue) through the §6 pipeline.
                let Some(r) = &result else { return };
                let at_seq = r.get("atSeq").and_then(Value::as_u64).unwrap_or(0);
                let at_epoch = r
                    .get("atLogEpoch")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let max_row_id = r.get("rows").and_then(Value::as_array).and_then(|rows| {
                    rows.iter()
                        .filter_map(|v| v.get("rowId").and_then(Value::as_u64))
                        .max()
                });
                let Some(c) = self.conversations.get_mut(&sid) else {
                    return;
                };
                // If the mirror's seq advanced since the previous poll, our own
                // deltas are flowing (we host the turn) — the atSeq comparison
                // would false-positive on every in-flight frame and is skipped.
                let deltas_flowing = c.seq != c.prev_poll_seq;
                c.prev_poll_seq = c.seq;
                let newer_row = max_row_id
                    .is_some_and(|id| c.rows.last_key_value().is_none_or(|(k, _)| id > *k));
                let epoch_moved = at_epoch.is_some_and(|e| c.log_epoch.as_ref() != Some(&e));
                let seq_moved = !deltas_flowing && at_seq != 0 && at_seq != c.seq;
                if newer_row || epoch_moved || seq_moved {
                    self.push_log(format!(
                        "poll detected movement (newer_row={newer_row}, epoch={epoch_moved}, seq={seq_moved}) -> resync"
                    ));
                    let topic = format!("conversation/{sid}");
                    self.resync_topic(ws_key, &topic);
                }
            }
            Pending::FetchUsageStats(range) => {
                if let Some(res) = &result {
                    self.usage_stats = Some(
                        crate::shared::usage_stats::AppUsageSnapshot::from_value(res, &range),
                    );
                    self.push_log(format!("usage stats loaded ({range})"));
                }
            }
            Pending::FetchMcpList => {
                if let Some(res) = &result {
                    self.mcp_servers = crate::shared::mcp::McpServerSnapshot::list_from_value(res);
                    self.push_log(format!(
                        "mcp servers loaded: {} found",
                        self.mcp_servers.len()
                    ));
                }
            }
            Pending::FetchPluginsOverview => {
                if let Some(res) = &result {
                    self.plugins_overview = Some(
                        crate::shared::plugins::PluginsOverviewResult::from_value(res),
                    );
                    self.push_log(format!(
                        "plugins overview loaded: {} available, {} installed",
                        self.plugins_overview
                            .as_ref()
                            .map(|o| o.available_plugins.len())
                            .unwrap_or(0),
                        self.plugins_overview
                            .as_ref()
                            .map(|o| o.installed_plugins.len())
                            .unwrap_or(0),
                    ));
                }
            }
            Pending::PluginAction(desc) => {
                self.push_log(format!("plugin action completed: {desc}"));
                self.fetch_plugins_overview(cx);
            }
        }
        cx.notify();
    }

    /// Subscribe acks carry the subscriptionId; resync needs it per topic.
    fn capture_subscription(&mut self, ws_key: &str, topic: &str, result: &Option<Value>) {
        let sub = result
            .as_ref()
            .and_then(|r| r.get("ack"))
            .and_then(|a| a.get("subscriptionId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(sub) = sub {
            let full_topic = match topic {
                "sessions-index" | "workspace-config" => {
                    let key = self.ws(ws_key).map(|w| w.key.clone()).unwrap_or_default();
                    format!("{topic}/{key}")
                }
                other => other.to_string(),
            };
            if let Some(ws) = self.ws_mut(ws_key) {
                ws.subscriptions.insert(full_topic, sub);
            }
        }
    }
}
