//! Desktop gateway state in `AppState`: the single owner of the pairing
//! store, the listener and every phone socket with its `RemoteScope`.
//! Listener threads report events here; routing happens only here.

use crate::app::store::AppState;
use crate::remote::handshake::{DesktopReply, PhoneHello, RemoteWorkspace};
use crate::remote::listener::{GatewayEvent, ListenerHandle, PhoneOutbound};
use crate::remote::pairing::PairingStore;
use crate::remote::scope::{FromPhone, RemoteScope, catalog_reply, is_remote_id};
use gpui::Context;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

struct OpenScope {
    ws_key: String,
    scope: RemoteScope,
    ready_sent: bool,
}

struct PhoneConn {
    outbound: PhoneOutbound,
    open: Option<OpenScope>,
}

pub(crate) struct PhoneGateway {
    pub(crate) store: Arc<Mutex<PairingStore>>,
    pub(super) listener: Option<ListenerHandle>,
    pub(super) fingerprint: Option<String>,
    conns: HashMap<u64, PhoneConn>,
    /// Per workspace: subscription ids of closed phone scopes whose late
    /// frames must not reach the desktop's assembler. Cleared with the CLI.
    retired: HashMap<String, HashSet<String>>,
    /// Phone model-catalog probes waiting for the desktop's own probe on a
    /// workspace whose agent has just started: (conn, workspace, phone id).
    catalog_waiters: Vec<(u64, String, Value)>,
    /// The latest pairing link, shown on Settings › Phone.
    pub(crate) link: Option<String>,
    pub(crate) error: Option<String>,
}

impl PhoneGateway {
    pub(crate) fn new(path: Option<std::path::PathBuf>) -> Self {
        Self {
            store: Arc::new(Mutex::new(PairingStore::load(path))),
            listener: None,
            fingerprint: None,
            conns: HashMap::new(),
            retired: HashMap::new(),
            catalog_waiters: Vec::new(),
            link: None,
            error: None,
        }
    }

    pub(crate) fn address(&self) -> Option<SocketAddr> {
        self.listener.as_ref().map(|l| l.addr)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.store.lock().is_ok_and(|s| s.file.enabled)
    }

    pub(crate) fn paired_device(&self) -> Option<String> {
        self.store
            .lock()
            .ok()
            .and_then(|s| s.file.device.as_ref().map(|d| d.name.clone()))
    }

    pub(crate) fn connected_sockets(&self) -> usize {
        self.conns.values().filter(|c| c.open.is_some()).count()
    }

    pub(crate) fn has_open(&self, ws_key: &str) -> bool {
        self.conns
            .values()
            .any(|c| c.open.as_ref().is_some_and(|o| o.ws_key == ws_key))
    }
}

fn ready_line() -> String {
    json!({"method": "startup/storageState", "params": {"schemaVersion": 1, "phase": "ready"}})
        .to_string()
}

impl AppState {
    fn remote_workspace_list(&self) -> Vec<RemoteWorkspace> {
        self.workspaces
            .iter()
            .map(|w| RemoteWorkspace {
                key: w.key.clone(),
                display: w.display.clone(),
                purpose: match w.purpose {
                    crate::backend::workspace::WorkspacePurpose::Conversation => "conversation",
                    crate::backend::workspace::WorkspacePurpose::Project => "project",
                }
                .into(),
            })
            .collect()
    }

    pub(super) fn handle_gateway_event(&mut self, event: GatewayEvent, cx: &mut Context<Self>) {
        match event {
            GatewayEvent::Authed {
                conn,
                outbound,
                device,
            } => {
                let list = DesktopReply::Workspaces {
                    workspaces: self.remote_workspace_list(),
                }
                .line();
                if outbound.send(list) {
                    self.phone.conns.insert(
                        conn,
                        PhoneConn {
                            outbound,
                            open: None,
                        },
                    );
                    self.push_log(format!("phone connected: {device}"));
                }
            }
            GatewayEvent::Line { conn, line } => self.handle_phone_line(conn, line, cx),
            GatewayEvent::Closed { conn } => self.close_phone_socket(conn),
        }
        cx.notify();
    }

    fn handle_phone_line(&mut self, conn: u64, line: String, cx: &mut Context<Self>) {
        let Some(open_key) = self
            .phone
            .conns
            .get(&conn)
            .map(|c| c.open.as_ref().map(|o| o.ws_key.clone()))
        else {
            return;
        };
        let Some(ws_key) = open_key else {
            self.open_phone_workspace(conn, &line, cx);
            return;
        };
        let settings = self.ws(&ws_key).and_then(|w| w.model_settings.clone());
        let Some(open) = self
            .phone
            .conns
            .get_mut(&conn)
            .and_then(|c| c.open.as_mut())
        else {
            return;
        };
        match open.scope.filter_phone_line(&line, settings.as_ref()) {
            FromPhone::Forward(out) => {
                if let Some(ws) = self.ws_mut(&ws_key) {
                    ws.last_activity = Some(std::time::Instant::now());
                    if !ws.send_line(out) {
                        self.close_phone_socket(conn);
                    }
                }
            }
            FromPhone::Reply(reply) => self.send_to_phone(conn, reply),
            FromPhone::AwaitCatalog(id) => self.phone.catalog_waiters.push((conn, ws_key, id)),
            FromPhone::Ignore => {}
        }
    }

    /// The desktop's model-catalog probe for `ws_key` returned: cache its
    /// `settings` for phones and answer the ones that asked before it.
    pub(crate) fn cache_model_settings(&mut self, ws_key: &str, settings: &Value) {
        if let Some(ws) = self.ws_mut(ws_key) {
            ws.model_settings = Some(settings.clone());
        }
        self.answer_phone_catalog_waiters(ws_key);
    }

    fn answer_phone_catalog_waiters(&mut self, ws_key: &str) {
        let Some(settings) = self.ws(ws_key).and_then(|w| w.model_settings.clone()) else {
            return;
        };
        let (ready, waiting) = std::mem::take(&mut self.phone.catalog_waiters)
            .into_iter()
            .partition(|(_, key, _)| key == ws_key);
        self.phone.catalog_waiters = waiting;
        for (conn, _, id) in ready {
            if self.phone.conns.contains_key(&conn) {
                self.send_to_phone(conn, catalog_reply(id, &settings));
            }
        }
    }

    fn open_phone_workspace(&mut self, conn: u64, line: &str, cx: &mut Context<Self>) {
        let key = match serde_json::from_str::<PhoneHello>(line) {
            Ok(PhoneHello::Open { workspace_key }) if self.ws(&workspace_key).is_some() => {
                workspace_key
            }
            _ => {
                self.send_to_phone(conn, DesktopReply::error("unknown_workspace"));
                self.close_phone_socket(conn);
                return;
            }
        };
        if let Some(c) = self.phone.conns.get_mut(&conn) {
            c.open = Some(OpenScope {
                ws_key: key.clone(),
                scope: RemoteScope::new(&key),
                ready_sent: false,
            });
        }
        self.send_to_phone(conn, DesktopReply::Opened.line());
        self.ensure_spawned(&key, cx);
        if self.ws(&key).is_some_and(|w| w.started) {
            self.phone_workspace_ready(&key);
        }
    }

    /// The workspace's CLI reached storage ready: phones waiting on it may
    /// start subscribing.
    pub(crate) fn phone_workspace_ready(&mut self, ws_key: &str) {
        let waiting: Vec<u64> = self
            .phone
            .conns
            .iter_mut()
            .filter_map(|(id, c)| {
                let open = c.open.as_mut()?;
                (open.ws_key == ws_key && !open.ready_sent).then(|| {
                    open.ready_sent = true;
                    *id
                })
            })
            .collect();
        for conn in waiting {
            self.send_to_phone(conn, ready_line());
        }
    }

    fn send_to_phone(&mut self, conn: u64, line: String) {
        let sent = self
            .phone
            .conns
            .get(&conn)
            .is_some_and(|c| c.outbound.send(line));
        if !sent {
            self.close_phone_socket(conn);
        }
    }

    /// Route a CLI stdout line that belongs to a phone. Returns true when the
    /// line was consumed and must not reach the desktop's own handling.
    pub(crate) fn route_cli_line_to_phone(&mut self, ws_key: &str, line: &str) -> bool {
        let retired = self
            .phone
            .retired
            .get(ws_key)
            .is_some_and(|r| !r.is_empty());
        if !retired && !self.phone.has_open(ws_key) {
            return false;
        }
        let Ok(Value::Object(msg)) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let method = msg.get("method").and_then(Value::as_str);
        if method.is_none()
            && let Some(id) = msg.get("id").filter(|id| is_remote_id(id))
        {
            let id = id.as_u64().unwrap_or_default();
            let owner = self.phone.conns.iter_mut().find_map(|(conn, c)| {
                let open = c.open.as_mut()?;
                (open.ws_key == ws_key && open.scope.owns_request(id)).then_some((*conn, open))
            });
            if let Some((conn, open)) = owner
                && let Some(out) = open.scope.take_response(id, msg)
            {
                self.send_to_phone(conn, out);
            }
            // Responses to closed scopes and cleanup requests are dropped.
            return true;
        }
        if method != Some("v4/conversation/frame") {
            return false;
        }
        let Some(sub) = msg
            .get("params")
            .and_then(|p| p.get("subscriptionId"))
            .and_then(Value::as_str)
        else {
            return false;
        };
        let owner = self.phone.conns.iter().find_map(|(conn, c)| {
            let open = c.open.as_ref()?;
            (open.ws_key == ws_key && open.scope.owns_subscription(sub)).then_some(*conn)
        });
        if let Some(conn) = owner {
            self.send_to_phone(conn, line.to_string());
            return true;
        }
        self.phone
            .retired
            .get(ws_key)
            .is_some_and(|r| r.contains(sub))
    }

    /// Close one phone socket and release what it held on the CLI.
    fn close_phone_socket(&mut self, conn: u64) {
        self.phone
            .catalog_waiters
            .retain(|(waiter, _, _)| *waiter != conn);
        let Some(c) = self.phone.conns.remove(&conn) else {
            return;
        };
        let Some(open) = c.open else {
            return;
        };
        let (lines, subs) = open.scope.close();
        self.phone
            .retired
            .entry(open.ws_key.clone())
            .or_default()
            .extend(subs);
        if let Some(ws) = self.ws_mut(&open.ws_key)
            && ws.inbound.is_some()
        {
            for line in lines {
                ws.send_line(line);
            }
        }
    }

    /// The workspace's CLI is gone (exit, restart or idle unload): its phone
    /// sockets close, and the phones reconnect and subscribe afresh.
    pub(crate) fn close_phone_sockets_for(&mut self, ws_key: &str) {
        self.phone
            .conns
            .retain(|_, c| c.open.as_ref().is_none_or(|o| o.ws_key != ws_key));
        self.phone.retired.remove(ws_key);
    }

    pub(super) fn close_all_phone_sockets(&mut self) {
        let conns: Vec<u64> = self.phone.conns.keys().copied().collect();
        for conn in conns {
            self.close_phone_socket(conn);
        }
    }
}

#[cfg(test)]
#[path = "gateway_tests.rs"]
mod tests;
