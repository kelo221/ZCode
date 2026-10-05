//! Connection event handling: pumps per workspace agent (with producer-side
//! backpressure signaling), wire routing, fault recovery (resync) and
//! auto-restart. Frame application lives in backend/route.rs; response
//! correlation lives in backend/responses.rs.

use crate::app::store::AppState;
use crate::backend::conn::EventBacklog;
use crate::backend::launcher::ConnEvent;
use futures::StreamExt;
use gpui::{AppContext, Context};
use serde_json::Value;

/// Producer-side backlog at which we tell the CLI to stop streaming frames.
const FLOW_HIGH: usize = 2000;
/// Backlog at which streaming may resume.
const FLOW_LOW: usize = 200;
/// Consecutive agent crashes before we give up and ask the user to reconnect.
const MAX_RESTARTS: u32 = 3;
/// Raw lines above twice the physical frame budget are never parsed — the
/// assembler would reject them anyway, so skip the allocation up front.
const MAX_RAW_LINE_BYTES: usize = crate::backend::wire::MAX_FRAME_BYTES * 2;

/// Attach the event pump for a workspace's agent connection. Backlog is
/// counted on the producer side (each enqueued event), so the flow signal
/// reflects real queue depth even when the serial consumer lags behind
/// (2026-10-05 audit P0.3).
pub(crate) fn attach_pump(
    cx: &mut Context<AppState>,
    ws_key: String,
    backlog: EventBacklog,
    mut events: futures::channel::mpsc::UnboundedReceiver<ConnEvent>,
) {
    cx.spawn(async move |this, cx| {
        while let Some(ev) = events.next().await {
            // Depth includes everything the producer queued and we have not
            // released yet — the true producer backlog.
            if backlog.depth() >= FLOW_HIGH {
                let key = ws_key.clone();
                let _ = this.update(cx, |s, _| s.flow_latch(&key, true));
            }
            let len = event_len(&ev);
            let is_exit = matches!(ev, ConnEvent::Exited);
            let keep_going = this
                .update(cx, |state, cx| state.handle_conn(&ws_key, ev, cx))
                .unwrap_or(false);
            // Exited bypassed producer admission, so it is never released.
            let depth = if is_exit {
                backlog.depth()
            } else {
                backlog.release(len)
            };
            if depth <= FLOW_LOW {
                let key = ws_key.clone();
                let _ = this.update(cx, |s, _| s.flow_latch(&key, false));
            }
            // Overflow means data lines were dropped at the hard byte bound;
            // the mirrored state is now holey — resync every route.
            if backlog.take_overflow() && keep_going {
                let key = ws_key.clone();
                let _ = this.update(cx, |s, _| s.resync_all(&key));
            }
            if !keep_going {
                break;
            }
        }
    })
    .detach();
}

fn event_len(ev: &ConnEvent) -> usize {
    match ev {
        ConnEvent::Line(l) | ConnEvent::Log(l) => l.len(),
        ConnEvent::Exited => 0,
    }
}

impl AppState {
    pub(crate) fn handle_conn(
        &mut self,
        ws_key: &str,
        ev: ConnEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        match ev {
            ConnEvent::Line(line) => self.handle_line(ws_key, line, cx),
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
                    if let Some(backlog) = self.ws(ws_key).and_then(|w| w.backlog.clone()) {
                        attach_pump(cx, ws_key.to_string(), backlog, events);
                    }
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
    /// pending rpcs, subscription ids, route cursors, fragment state.
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
        // Cursors AND fragment state die with the connection: a stale frame
        // from the old generation must never be applied to the new one
        // (2026-10-05 audit P0.1, "clear on route replacement").
        for topic in &stale {
            self.route_cursors.remove(topic);
        }
        self.assembler.forget_topics(&stale);
        for sid in sids {
            if let Some(c) = self.conversations.get_mut(&sid) {
                c.subscribed = false;
            }
        }
    }

    fn handle_line(&mut self, ws_key: &str, line: String, cx: &mut Context<Self>) -> bool {
        if line.len() > MAX_RAW_LINE_BYTES {
            self.push_log(format!(
                "dropped oversized line ({} bytes > {MAX_RAW_LINE_BYTES})",
                line.len()
            ));
            return true;
        }
        let Some(incoming) = crate::backend::wire::parse_line(&line) else {
            self.push_log(format!("unparsable stdout line: {line:.120}"));
            return true;
        };
        match incoming {
            crate::backend::wire::Incoming::AgentRequest { id, method, params } => {
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
            crate::backend::wire::Incoming::Notification { method, params } => {
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
            crate::backend::wire::Incoming::Response { id, result, error } => {
                self.handle_response(ws_key, id, result, error, cx);
                true
            }
        }
    }
}
