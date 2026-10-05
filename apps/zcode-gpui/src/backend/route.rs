//! Route application for V4 topic frames: subscription identity and seq
//! continuity (the client half of packages/shared/src/zcode-protocol-v4/
//! controller.ts `isWindowHostControllerFrameGap` + transport.ts frame rules).
//!
//! Deltas cover `(fromSeq, toSeq]` and must start exactly at the applied
//! cursor's seq — `fromSeq == cursor.seq + 1` is a one-event gap, not
//! contiguous (2026-10-05 audit P0.1). Snapshots are authoritative and
//! re-establish the cursor. Frames from a superseded subscription generation
//! are dropped; a gap or missing base triggers resync and never mutates
//! mirrored state.

use crate::app::store::{AppState, RouteCursor};
use crate::backend::wire::LogicalFrame;
use crate::conversation::model::SessionEntry;
use gpui::Context;
use serde_json::Value;

/// What the route layer decided to do with a frame. Pure function — golden
/// tested in route_tests.rs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrameDecision {
    /// Apply the payload, then advance the cursor to `toSeq`.
    Apply,
    /// Stale / duplicate / old-generation frame: drop silently.
    Drop,
    /// Discontinuity, no base, or malformed range: resync the route and do
    /// not touch any mirrored state.
    Resync,
}

/// `cursor` is `(subscriptionId, seq)` of the last applied frame on the topic;
/// `active_sub` is the subscription id the client currently holds (from the
/// subscribe ack), if any.
pub(crate) fn classify_frame(
    cursor: Option<(&str, u64)>,
    active_sub: Option<&str>,
    kind: &str,
    frame_sub: &str,
    from_seq: u64,
    to_seq: u64,
) -> FrameDecision {
    if to_seq < from_seq {
        return FrameDecision::Resync;
    }
    match kind {
        "snapshot" => {
            if active_sub.is_some_and(|s| s != frame_sub) {
                // An old generation's snapshot would roll live state back.
                return FrameDecision::Drop;
            }
            // Canonical frame schema: snapshots start at seq zero.
            if from_seq != 0 {
                return FrameDecision::Resync;
            }
            FrameDecision::Apply
        }
        "deltas" => {
            let from_applied_generation = cursor.is_some_and(|(sub, _)| sub == frame_sub);
            if !from_applied_generation {
                // Deltas from a generation we never applied: if it is the
                // active subscription we missed its snapshot → resync;
                // otherwise it is an old-generation leftover → drop.
                return if active_sub.is_some_and(|s| s == frame_sub) {
                    FrameDecision::Resync
                } else {
                    FrameDecision::Drop
                };
            }
            let (_, seq) = cursor.expect("checked above");
            if to_seq <= seq {
                // Duplicate or fully-superseded frame: never re-applied.
                FrameDecision::Drop
            } else if from_seq != seq {
                // Gap (including the one-event gap `fromSeq == seq + 1`) and
                // partial overlap alike: the baseline is untrustworthy.
                FrameDecision::Resync
            } else {
                FrameDecision::Apply
            }
        }
        // Unknown payload kinds are tolerated (never applied): PARITY.md §6.
        _ => FrameDecision::Drop,
    }
}

impl AppState {
    pub(crate) fn route_frame(&mut self, frame: LogicalFrame, cx: &mut Context<Self>) {
        let kind = frame
            .payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let active_sub = self.active_subscription_for(&frame.topic);
        let cursor = self
            .route_cursors
            .get(&frame.topic)
            .map(|c| (c.subscription_id.as_str(), c.seq));
        match classify_frame(
            cursor,
            active_sub.as_deref(),
            &kind,
            &frame.subscription_id,
            frame.from_seq,
            frame.to_seq,
        ) {
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
                self.apply_payload(&frame, &kind);
                self.advance_cursor(&frame, &kind);
            }
        }
        cx.notify();
    }

    fn active_subscription_for(&self, topic: &str) -> Option<String> {
        let ws_key = self.ws_for_topic(topic)?;
        self.ws(&ws_key)
            .and_then(|w| w.subscriptions.get(topic))
            .map(|s| s.id.clone())
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

    /// Dispatch an approved frame payload to its topic reducer. Infallible by
    /// design: every fallible check already happened in the assembler and in
    /// `classify_frame`, so state mutation only ever follows validation.
    fn apply_payload(&mut self, frame: &LogicalFrame, kind: &str) {
        if let Some(ws_key) = frame.topic.strip_prefix("sessions-index/") {
            self.apply_sessions_index(ws_key, kind, &frame.payload);
        } else if let Some(ws_key) = frame.topic.strip_prefix("workspace-config/") {
            self.apply_workspace_config(ws_key, kind, &frame.payload);
        } else if let Some(sid) = frame.topic.strip_prefix("conversation/") {
            let state: &mut crate::conversation::model::ConversationState =
                self.conversations.entry(sid.to_string()).or_default();
            match kind {
                "snapshot" => {
                    state.subscribed = true;
                    if let Some(snap) = frame.payload.get("snapshot") {
                        state.apply_snapshot(snap);
                    }
                }
                "deltas" => {
                    if let Some(deltas) = frame.payload.get("deltas").and_then(Value::as_array) {
                        state.apply_deltas(deltas);
                    }
                }
                _ => {}
            }
        }
    }

    fn apply_workspace_config(&mut self, ws_key: &str, kind: &str, payload: &Value) {
        let Some(cfg) = self.workspace_configs.get_mut(ws_key) else {
            return;
        };
        match kind {
            "snapshot" => {
                if let Some(config) = payload.get("snapshot").and_then(|s| s.get("config")) {
                    cfg.apply_state(config);
                }
            }
            "deltas" => {
                if let Some(deltas) = payload.get("deltas").and_then(Value::as_array) {
                    for d in deltas {
                        if d.get("op").and_then(Value::as_str) == Some("config.updated")
                            && let Some(config) = d.get("config")
                        {
                            cfg.apply_state(config);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn apply_sessions_index(&mut self, ws_key: &str, kind: &str, payload: &Value) {
        let first_sid = match kind {
            "snapshot" => {
                let Some(ws) = self.ws_mut(ws_key) else {
                    return;
                };
                let mut list: Vec<SessionEntry> = payload
                    .get("snapshot")
                    .and_then(|s| s.get("sessions"))
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(SessionEntry::from_value).collect())
                    .unwrap_or_default();
                list.sort_by_key(|s| std::cmp::Reverse(s.last_activity_at));
                let first = list.first().map(|s| s.session_id.clone());
                ws.sessions = list;
                first
            }
            "deltas" => {
                let Some(ws) = self.ws_mut(ws_key) else {
                    return;
                };
                let Some(deltas) = payload.get("deltas").and_then(Value::as_array) else {
                    return;
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
    }
}

#[cfg(test)]
#[path = "route_tests.rs"]
mod tests;
