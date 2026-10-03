//! Connection event handling: pumps per workspace agent (with backpressure
//! signaling), wire routing, fault recovery (resync) and auto-restart.
//! Response correlation lives in backend/responses.rs.

use crate::app::store::AppState;
use crate::backend::launcher::ConnEvent;
use crate::backend::wire::Incoming;
use crate::conversation::model::{ConversationState, SessionEntry};
use futures::StreamExt;
use gpui::{AppContext, Context};
use serde_json::Value;
use std::sync::atomic::Ordering;

/// Queue depth at which we tell the CLI to stop streaming frames.
const FLOW_HIGH: usize = 2000;
/// Queue depth at which streaming may resume.
const FLOW_LOW: usize = 200;
/// Consecutive agent crashes before we give up and ask the user to reconnect.
const MAX_RESTARTS: u32 = 3;

/// Attach the event pump for a workspace's agent connection. The pump counts
/// its backlog and signals `v4/connection/flow` at the thresholds so the CLI
/// pauses frame emission when we can't keep up.
pub(crate) fn attach_pump(
    cx: &mut Context<AppState>,
    ws_key: String,
    pending: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    mut events: futures::channel::mpsc::UnboundedReceiver<ConnEvent>,
) {
    // Note: the counter is passed in — reading the entity here would panic
    // when attach happens during the entity's own update (constructor/restart).
    cx.spawn(async move |this, cx| {
        while let Some(ev) = events.next().await {
            let depth = pending.fetch_add(1, Ordering::Relaxed) + 1;
            if depth == FLOW_HIGH {
                let key = ws_key.clone();
                let _ = this.update(cx, |s, _| s.flow_latch(&key, true));
            }
            let keep_going = this
                .update(cx, |state, cx| state.handle_conn(&ws_key, ev, cx))
                .unwrap_or(false);
            let depth = pending.fetch_sub(1, Ordering::Relaxed) - 1;
            if depth == FLOW_LOW {
                let key = ws_key.clone();
                let _ = this.update(cx, |s, _| s.flow_latch(&key, false));
            }
            if !keep_going {
                break;
            }
        }
    })
    .detach();
}

impl AppState {
    pub(crate) fn handle_conn(
        &mut self,
        ws_key: &str,
        ev: ConnEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match ev {
            ConnEvent::Line(line) => self.handle_line(ws_key, &line, cx),
            ConnEvent::Log(l) => {
                // Agent stderr can carry provider request dumps; scrub before
                // it reaches the console or the in-memory log.
                let l = crate::shared::redact::scrub(&l);
                eprintln!("[zcode-gpui:{ws_key}:stderr] {l}");
                self.push_log(l);
                cx.notify();
                true
            }
            ConnEvent::Exited => self.handle_exit(ws_key, cx),
        }
    }

    /// Agent process died. An idle unload is expected (no respawn); otherwise
    /// auto-restart with linear backoff up to MAX_RESTARTS, then surface a
    /// reconnect error. Conversation mirrors survive — the fresh snapshot
    /// delivered on re-subscribe rebuilds them.
    fn handle_exit(&mut self, ws_key: &str, cx: &mut Context<Self>) -> bool {
        let Some(ws) = self.ws_mut(ws_key) else {
            return false;
        };
        ws.kill = None;
        if ws.status.starts_with("idle") {
            // Deliberate unload: no respawn, the pump ends here.
            ws.pumping = false;
            cx.notify();
            return true;
        }
        if !ws.started {
            // Candidate failed before storage startup: next candidate,
            // otherwise give up on this workspace.
            ws.inbound = None;
            let display = ws.display.clone();
            match ws.try_spawn() {
                Some(events) => {
                    eprintln!(
                        "[zcode-gpui] backend for {ws_key} exited before startup, trying next candidate"
                    );
                    self.push_log(format!(
                        "backend for {ws_key} died before startup, trying next candidate"
                    ));
                    let pending = self.inflight_events.clone();
                    attach_pump(cx, ws_key.to_string(), pending, events);
                }
                None => {
                    ws.status = "no backend".into();
                    eprintln!("[zcode-gpui] no usable backend for {display}");
                    self.push_error(format!(
                        "no usable backend for {display} — check the installed ZCode app"
                    ));
                }
            }
            cx.notify();
            return true;
        }
        self.reset_connection_state(ws_key);
        let attempts = self.ws(ws_key).map(|w| w.restart_attempts).unwrap_or(0);
        if attempts >= MAX_RESTARTS {
            if let Some(ws) = self.ws_mut(ws_key) {
                ws.status = "backend down".into();
            }
            self.push_error(format!(
                "agent for {} keeps exiting — click the project to reconnect",
                self.ws(ws_key)
                    .map(|w| w.display.clone())
                    .unwrap_or_default()
            ));
            cx.notify();
            return true;
        }
        let delay_ms = 1000u64 * (attempts as u64 + 1);
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.restart_attempts += 1;
            ws.status = format!("restarting (attempt {}/{})", attempts + 1, MAX_RESTARTS);
        }
        self.push_log(format!(
            "agent for {ws_key} exited; restarting in {delay_ms}ms"
        ));
        let key = ws_key.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_spawn(async move {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            })
            .await;
            let _ = this.update(cx, |state, cx| state.spawn_workspace(&key, cx));
        })
        .detach();
        cx.notify();
        true
    }

    /// Clear per-connection mirrors so a fresh subscribe rebuilds them:
    /// pending rpcs, subscription ids, seq tracking, subscribed flags.
    pub(crate) fn reset_connection_state(&mut self, ws_key: &str) {
        let sids: Vec<String> = match self.ws_mut(ws_key) {
            Some(ws) => {
                ws.inbound = None;
                ws.started = false;
                ws.pumping = false; // the old pump is ending; allow respawn
                ws.prepare_respawn();
                ws.pending.clear();
                ws.subscriptions.clear();
                ws.desired_conversation = None;
                ws.sessions.iter().map(|s| s.session_id.clone()).collect()
            }
            None => return,
        };
        let mut stale = vec![
            format!("sessions-index/{ws_key}"),
            format!("workspace-config/{ws_key}"),
        ];
        for sid in &sids {
            stale.push(format!("conversation/{sid}"));
        }
        for t in stale {
            self.last_seq.remove(&t);
        }
        for sid in sids {
            if let Some(c) = self.conversations.get_mut(&sid) {
                c.subscribed = false;
            }
        }
    }

    fn handle_line(&mut self, ws_key: &str, line: &str, cx: &mut Context<Self>) -> bool {
        let Some(incoming) = crate::backend::wire::parse_line(line) else {
            self.push_log(format!("unparsable stdout line: {line:.120}"));
            return true;
        };
        match incoming {
            Incoming::AgentRequest { id, method, params } => {
                let action =
                    crate::backend::reverse_rpc::dispatch_reverse_rpc(&id, &method, &params);
                match action {
                    crate::backend::reverse_rpc::ReverseRpcAction::Raced => true,
                    crate::backend::reverse_rpc::ReverseRpcAction::Respond(resp) => {
                        if let Some(ws) = self.ws_mut(ws_key) {
                            ws.send_line(resp);
                        }
                        true
                    }
                }
            }
            Incoming::Notification { method, params } => {
                match method.as_str() {
                    "startup/storageState" => {
                        let phase = params.get("phase").and_then(Value::as_str).unwrap_or("");
                        if phase == "ready" {
                            let already = self.ws(ws_key).map(|w| w.started).unwrap_or(true);
                            if !already {
                                if let Some(ws) = self.ws_mut(ws_key) {
                                    ws.started = true;
                                    ws.status = "connected".into();
                                    ws.restart_attempts = 0;
                                    ws.last_activity = Some(std::time::Instant::now());
                                }
                                self.subscribe_index(ws_key);
                                self.subscribe_config(ws_key);
                                self.probe_model_catalog(ws_key);
                                // Cold-open: the user already picked a session
                                // while this agent was starting.
                                let desired = self
                                    .ws_mut(ws_key)
                                    .and_then(|w| w.desired_conversation.take());
                                if let Some(sid) = desired {
                                    self.subscribe_conversation(ws_key, &sid);
                                }
                            }
                        }
                    }
                    "v4/conversation/frame" => match self.assembler.ingest(&params) {
                        Ok(Some(frame)) => self.route_frame(frame, cx),
                        Ok(None) => {}
                        Err(e) => {
                            // Corrupt frame (CRC/schema/ordinal conflict):
                            // request a fresh snapshot for that route.
                            let topic = params
                                .get("topic")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            self.push_log(format!("frame error: {e}"));
                            self.resync_topic(ws_key, &topic);
                        }
                    },
                    _ => {}
                }
                true
            }
            Incoming::Response { id, result, error } => {
                self.handle_response(ws_key, id, result, error, cx);
                true
            }
        }
    }

    fn route_frame(&mut self, frame: crate::backend::wire::LogicalFrame, cx: &mut Context<Self>) {
        // Seq continuity check: frames cover (fromSeq, toSeq]. A gap means we
        // missed frames — deltas can no longer be applied reliably, so ask the
        // server for a fresh snapshot on that route.
        let last = self.last_seq.get(&frame.topic).copied();
        if let Some(last) = last
            && frame.from_seq > last + 1
        {
            self.push_log(format!(
                "seq gap on {}: expected ≤{}, got {}-{}",
                frame.topic,
                last + 1,
                frame.from_seq,
                frame.to_seq
            ));
            if let Some(ws_key) = self.ws_for_topic(&frame.topic) {
                self.resync_topic(&ws_key, &frame.topic);
            }
        }
        self.last_seq.insert(frame.topic.clone(), frame.to_seq);

        let kind = frame
            .payload
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if let Some(ws_key) = frame.topic.strip_prefix("sessions-index/") {
            self.apply_sessions_index(ws_key, &kind, &frame.payload);
        } else if let Some(ws_key) = frame.topic.strip_prefix("workspace-config/") {
            if let Some(cfg) = self.workspace_configs.get_mut(ws_key) {
                match kind.as_str() {
                    "snapshot" => {
                        if let Some(config) =
                            frame.payload.get("snapshot").and_then(|s| s.get("config"))
                        {
                            cfg.apply_state(config);
                        }
                    }
                    "deltas" => {
                        if let Some(deltas) = frame.payload.get("deltas").and_then(Value::as_array)
                        {
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
        } else if let Some(sid) = frame.topic.strip_prefix("conversation/") {
            let state: &mut ConversationState =
                self.conversations.entry(sid.to_string()).or_default();
            match kind.as_str() {
                "snapshot" => {
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
        cx.notify();
    }

    fn apply_sessions_index(&mut self, ws_key: &str, kind: &str, payload: &Value) {
        let Some(ws) = self.ws_mut(ws_key) else {
            return;
        };
        match kind {
            "snapshot" => {
                let mut sessions: Vec<SessionEntry> = payload
                    .get("snapshot")
                    .and_then(|s| s.get("sessions"))
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(SessionEntry::from_value).collect())
                    .unwrap_or_default();
                sessions.sort_by_key(|s| std::cmp::Reverse(s.last_activity_at));
                ws.sessions = sessions;
            }
            "deltas" => {
                let Some(deltas) = payload.get("deltas").and_then(Value::as_array) else {
                    return;
                };
                for d in deltas {
                    match d.get("op").and_then(Value::as_str).unwrap_or("") {
                        "session.upserted" => {
                            if let Some(entry) = d.get("session").and_then(SessionEntry::from_value)
                            {
                                if let Some(existing) = ws
                                    .sessions
                                    .iter_mut()
                                    .find(|s| s.session_id == entry.session_id)
                                {
                                    *existing = entry;
                                } else {
                                    ws.sessions.push(entry);
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
            }
            _ => {}
        }
    }
}
