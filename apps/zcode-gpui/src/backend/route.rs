//! Route application for V4 topic frames: the AppState half of the route
//! layer. The pure continuity/identity decision and payload-shape validation
//! live in backend/route_rules.rs (client half of
//! packages/shared/src/zcode-protocol-v4/controller.ts
//! `isWindowHostControllerFrameGap` + transport.ts frame rules). Deltas cover
//! `(fromSeq, toSeq]` and must start exactly at the applied cursor's seq; the
//! cursor advances only after a successful, complete application.

use crate::app::store::{AppState, RouteCursor};
use crate::backend::wire::LogicalFrame;
use crate::backend::workspace::RouteSubscription;
use crate::conversation::model::SessionEntry;
use gpui::Context;
use serde_json::Value;

use crate::backend::route_rules::{FrameDecision, classify_frame, validate_payload_shape};

impl AppState {
    pub(crate) fn route_frame(&mut self, frame: LogicalFrame, cx: &mut Context<Self>) {
        let kind = frame
            .payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let active: Option<RouteSubscription> = self.active_subscription_for(&frame.topic);
        let cursor = self
            .route_cursors
            .get(&frame.topic)
            .map(|c| (c.subscription_id.as_str(), c.log_epoch.as_str(), c.seq));
        let snapshot_epoch = frame
            .payload
            .get("snapshot")
            .and_then(|s| s.get("logEpoch"))
            .and_then(Value::as_str);
        let decision = classify_frame(
            cursor,
            active
                .as_ref()
                .map(|s| (s.id.as_str(), s.log_epoch.as_str())),
            &kind,
            &frame.subscription_id,
            frame.from_seq,
            frame.to_seq,
            snapshot_epoch,
        );
        match decision {
            FrameDecision::Drop => {}
            FrameDecision::Resync => {
                self.push_log(format!(
                    "route fault on {topic}: {kind} {from}-{to} does not continue the applied cursor",
                    topic = frame.topic,
                    from = frame.from_seq,
                    to = frame.to_seq
                ));
                if let Some(ws_key) = self.ws_for_topic(&frame.topic) {
                    self.resync_topic(&ws_key, &frame.topic);
                }
                // Deliberate early return: the payload is never applied and
                // the cursor keeps pointing at the last trusted frame.
                return;
            }
            FrameDecision::Apply => {
                // The cursor advances only after a successful, complete
                // application (review finding 3).
                if let Err(e) = self.apply_payload(&frame, &kind) {
                    self.push_log(format!("payload fault on {}: {e}", frame.topic));
                    if let Some(ws_key) = self.ws_for_topic(&frame.topic) {
                        self.resync_topic(&ws_key, &frame.topic);
                    }
                    return;
                }
                self.advance_cursor(&frame, &kind);
                if let Some(sid) = frame.topic.strip_prefix("conversation/")
                    && let Some(key) = self.ws_for_topic(&frame.topic)
                {
                    self.reconcile_workflow_settings(&key, sid, kind == "snapshot");
                }
            }
        }
        cx.notify();
    }

    fn active_subscription_for(&self, topic: &str) -> Option<RouteSubscription> {
        let ws_key = self.ws_for_topic(topic)?;
        self.ws(&ws_key)
            .and_then(|w| w.subscriptions.get(topic))
            .cloned()
    }

    /// Advance the cursor after a successful apply. Snapshots re-read the
    /// route's log epoch from the payload when present (the ack epoch is the
    /// fallback); deltas never touch it.
    fn advance_cursor(&mut self, frame: &LogicalFrame, kind: &str) {
        let epoch_from_snapshot = (kind == "snapshot")
            .then(|| {
                frame
                    .payload
                    .get("snapshot")
                    .and_then(|s| s.get("logEpoch"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let entry = self
            .route_cursors
            .entry(frame.topic.clone())
            .or_insert_with(|| RouteCursor {
                subscription_id: frame.subscription_id.clone(),
                log_epoch: String::new(),
                seq: 0,
            });
        entry.subscription_id = frame.subscription_id.clone();
        if let Some(epoch) = epoch_from_snapshot {
            entry.log_epoch = epoch;
        }
        entry.seq = frame.to_seq;
    }

    /// Validate, then dispatch an approved frame payload to its topic
    /// reducer. Every fallible check happens before any mutation, so `Err`
    /// guarantees untouched mirrored state.
    fn apply_payload(&mut self, frame: &LogicalFrame, kind: &str) -> Result<(), String> {
        validate_payload_shape(&frame.topic, kind, &frame.payload)?;
        if let Some(ws_key) = frame.topic.strip_prefix("sessions-index/") {
            self.apply_sessions_index(ws_key, kind, &frame.payload)
        } else if let Some(ws_key) = frame.topic.strip_prefix("workspace-config/") {
            self.apply_workspace_config(ws_key, kind, &frame.payload)
        } else if let Some(sid) = frame.topic.strip_prefix("conversation/") {
            let state: &mut crate::conversation::model::ConversationState =
                self.conversations.entry(sid.to_string()).or_default();
            match kind {
                "snapshot" => {
                    state.subscribed = true;
                    let snap = frame
                        .payload
                        .get("snapshot")
                        .ok_or_else(|| "snapshot payload missing 'snapshot'".to_string())?;
                    state.apply_snapshot(snap);
                    Ok(())
                }
                "deltas" => {
                    let deltas = frame
                        .payload
                        .get("deltas")
                        .and_then(Value::as_array)
                        .ok_or_else(|| "deltas payload missing deltas array".to_string())?;
                    state.apply_deltas(deltas);
                    Ok(())
                }
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    fn apply_workspace_config(
        &mut self,
        ws_key: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<(), String> {
        match kind {
            "snapshot" => {
                let config = payload
                    .get("snapshot")
                    .and_then(|s| s.get("config"))
                    .ok_or_else(|| "workspace-config snapshot missing config".to_string())?;
                self.workspace_configs
                    .entry(ws_key.to_string())
                    .or_default()
                    .apply_state(config);
                Ok(())
            }
            "deltas" => {
                // Deltas without a snapshot baseline are unappliable.
                let cfg = self
                    .workspace_configs
                    .get_mut(ws_key)
                    .ok_or_else(|| "workspace-config delta before any snapshot".to_string())?;
                let deltas = payload
                    .get("deltas")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "deltas payload missing deltas array".to_string())?;
                for d in deltas {
                    if d.get("op").and_then(Value::as_str) == Some("config.updated")
                        && let Some(config) = d.get("config")
                    {
                        cfg.apply_state(config);
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn apply_sessions_index(
        &mut self,
        ws_key: &str,
        kind: &str,
        payload: &Value,
    ) -> Result<(), String> {
        let first_sid = match kind {
            "snapshot" => {
                let Some(ws) = self.ws_mut(ws_key) else {
                    return Err("sessions-index for unknown workspace".into());
                };
                // `sessions` is validated to be an array; individual invalid
                // entries are skipped (forward compatibility).
                let mut list: Vec<SessionEntry> = payload
                    .get("snapshot")
                    .and_then(|s| s.get("sessions"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| "sessions-index snapshot missing sessions array".to_string())?
                    .iter()
                    .filter_map(SessionEntry::from_value)
                    .collect();
                list.sort_by_key(|s| std::cmp::Reverse(s.last_activity_at));
                let first = list.first().map(|s| s.session_id.clone());
                ws.sessions = list;
                first
            }
            "deltas" => {
                let Some(ws) = self.ws_mut(ws_key) else {
                    return Err("sessions-index for unknown workspace".into());
                };
                let Some(deltas) = payload.get("deltas").and_then(Value::as_array) else {
                    return Err("deltas payload missing deltas array".into());
                };
                for d in deltas {
                    match d.get("op").and_then(Value::as_str).unwrap_or("") {
                        "session.upserted" => {
                            if let Some(e) = d.get("session").and_then(SessionEntry::from_value) {
                                if let Some(s) = ws
                                    .sessions
                                    .iter_mut()
                                    .find(|s| s.session_id == e.session_id)
                                {
                                    *s = e;
                                } else {
                                    ws.sessions.push(e);
                                }
                            }
                        }
                        "session.removed" => {
                            if let Some(sid) = d.get("sessionId").and_then(Value::as_str) {
                                ws.sessions.retain(|s| s.session_id != sid);
                            }
                        }
                        _ => {}
                    }
                }
                ws.sort_sessions();
                None
            }
            _ => None,
        };
        if self.active.is_none()
            && !self.draft
            && let Some(sid) = first_sid
        {
            self.active = Some(sid.clone());
            self.subscribe_conversation(ws_key, &sid);
        }
        Ok(())
    }
}
