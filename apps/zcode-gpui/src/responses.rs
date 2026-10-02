//! Response correlation: pending-entry resolution, subscription-id capture,
//! CAS config acks (stale retry), history pages.

use crate::store::AppState;
use crate::workspace::Pending;
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
        let revision = result
            .as_ref()
            .and_then(|ack| ack.get("revisionAtDecision"))
            .and_then(Value::as_u64);
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
                if let Some(c) = self.conversations.get_mut(&sid) {
                    c.subscribed = true;
                }
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
            Pending::SendText | Pending::Stop => {
                // Keep the CAS revision fresh: non-CAS acks also carry the
                // decision revision.
                if let (Some(sid), Some(r)) = (&self.active, revision)
                    && let Some(c) = self.conversations.get_mut(sid)
                {
                    c.revision = r + 1;
                }
            }
            Pending::Command(ctx) => {
                self.handle_command_ack(ws_key, result.as_ref(), ctx);
            }
            Pending::Resync => {
                self.push_log("resync acknowledged".into());
            }
            Pending::FetchRows(sid) => {
                let (rows, has_more, at_rev) = match &result {
                    Some(r) => (
                        r.get("rows").and_then(Value::as_array).cloned(),
                        r.get("hasMore").and_then(Value::as_bool),
                        r.get("atRevision").and_then(Value::as_u64),
                    ),
                    None => (None, None, None),
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
                    if let Some(r) = at_rev {
                        c.revision = r;
                    }
                }
                self.push_log(format!("history page: +{count} rows"));
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
