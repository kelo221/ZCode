//! The bridge's state in `AppState`: the single owner of connected browsers
//! (descriptor id + generation) and of in-flight agent requests. Socket
//! threads only move lines; every decision below is synchronous and pure, so
//! the tests drive it without sockets or a window.

use super::commands::{
    browser_id, failure, may_have_side_effects, response_line, text, timeout_for, unix_millis,
};
use crate::remote::listener::PhoneOutbound as Outbound;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct BrowserInfo {
    pub(crate) id: String,
    pub(crate) generation: u64,
    pub(crate) name: String,
    pub(crate) profile: String,
    pub(crate) extension_version: String,
    connected_at: Instant,
}

struct Conn {
    outbound: Outbound,
    browser: Option<BrowserInfo>,
}

struct Pending {
    conn: u64,
    ws_key: String,
    rpc_id: Value,
    deadline: Instant,
    started: Instant,
    method: String,
    side_effects: bool,
}

/// A JSON-RPC response line for the agent of `ws_key`.
#[derive(Debug, PartialEq)]
pub(crate) struct Reply {
    pub(crate) ws_key: String,
    pub(crate) line: String,
}

#[derive(Default)]
pub(crate) struct BrowserBridge {
    conns: HashMap<u64, Conn>,
    pending: HashMap<String, Pending>,
    generations: HashMap<String, u64>,
    pub(crate) listener: Option<super::listener::BridgeListener>,
    pub(crate) error: Option<String>,
    /// Host registration as last read; refreshed when Settings opens it.
    pub(crate) registration: Option<super::registration::RegistrationStatus>,
    pub(crate) registration_error: Option<String>,
}

impl BrowserBridge {
    pub(crate) fn browsers(&self) -> Vec<&BrowserInfo> {
        let mut list: Vec<_> = self
            .conns
            .values()
            .filter_map(|c| c.browser.as_ref())
            .collect();
        list.sort_by_key(|b| std::cmp::Reverse(b.connected_at));
        list
    }

    pub(crate) fn has_pending(&self, ws_key: &str) -> bool {
        self.pending.values().any(|p| p.ws_key == ws_key)
    }

    /// Descriptors for `interaction/browserList`, newest connection first so
    /// an agent that takes the first entry gets the browser used last.
    pub(crate) fn descriptors(&self) -> Vec<Value> {
        self.browsers()
            .into_iter()
            .map(|b| {
                json!({
                    "id": b.id,
                    "generation": b.generation,
                    "type": "extension",
                    "name": format!("{} ({})", b.name, b.profile),
                    "capabilities": { "browser": [], "tab": [] },
                    "apiSupportOverrides": super::API_SUPPORT_OVERRIDES
                        .iter()
                        .map(|(k, v)| ((*k).to_string(), json!(v)))
                        .collect::<Map<_, _>>(),
                    "metadata": {
                        "provider": "zcode-gpui-extension",
                        "browser": b.name,
                        "extensionVersion": b.extension_version,
                    },
                })
            })
            .collect()
    }

    pub(crate) fn connected(&mut self, conn: u64, outbound: Outbound) {
        self.conns.insert(
            conn,
            Conn {
                outbound,
                browser: None,
            },
        );
    }

    /// The socket closed: its in-flight requests can no longer be answered.
    pub(crate) fn closed(&mut self, conn: u64, now: Instant) -> Vec<Reply> {
        self.conns.remove(&conn);
        let lost: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.conn == conn)
            .map(|(id, _)| id.clone())
            .collect();
        lost.into_iter()
            .filter_map(|id| self.pending.remove(&id))
            .map(|p| Reply {
                line: response_line(
                    &p.rpc_id,
                    failure(
                        "backend_unavailable",
                        "the browser extension disconnected before answering",
                        Some(if p.side_effects { "uncertain" } else { "none" }),
                        now - p.started,
                    ),
                ),
                ws_key: p.ws_key,
            })
            .collect()
    }

    pub(crate) fn handle_line(&mut self, conn: u64, line: &str, now: Instant) -> Vec<Reply> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match message["op"].as_str() {
            Some("hello") => self.hello(conn, &message, now),
            Some("result") => self.result(conn, &message, now).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    fn hello(&mut self, conn: u64, message: &Value, now: Instant) -> Vec<Reply> {
        let Some(id) = message["profileId"].as_str().and_then(browser_id) else {
            return Vec::new();
        };
        // A reconnect of the same profile replaces the old socket; whatever
        // was in flight there cannot be answered any more.
        let stale: Vec<u64> = self
            .conns
            .iter()
            .filter(|(c, x)| **c != conn && x.browser.as_ref().is_some_and(|b| b.id == id))
            .map(|(c, _)| *c)
            .collect();
        let mut replies = Vec::new();
        for old in stale {
            replies.extend(self.closed(old, now));
        }
        let previous = self.generations.get(&id).copied().unwrap_or(0);
        let generation = unix_millis().max(previous + 1);
        self.generations.insert(id.clone(), generation);
        let Some(entry) = self.conns.get_mut(&conn) else {
            return replies;
        };
        let info = BrowserInfo {
            id: id.clone(),
            generation,
            name: text(&message["browser"], "Chromium"),
            profile: text(&message["profile"], "Default"),
            extension_version: text(&message["extensionVersion"], "unknown"),
            connected_at: now,
        };
        entry.browser = Some(info);
        let welcome = json!({ "op": "welcome", "browserId": id, "generation": generation });
        entry.outbound.send(welcome.to_string());
        replies
    }

    fn result(&mut self, conn: u64, message: &Value, now: Instant) -> Option<Reply> {
        let request_id = message["requestId"].as_str()?;
        // Late answers (after a timeout) and answers from another socket are dropped.
        if self.pending.get(request_id).is_none_or(|p| p.conn != conn) {
            return None;
        }
        let p = self.pending.remove(request_id)?;
        let result = match &message["result"] {
            r if r["ok"].is_boolean() => r.clone(),
            _ => failure(
                "execution_error",
                "the browser extension returned a malformed result",
                Some(if p.side_effects { "uncertain" } else { "none" }),
                now - p.started,
            ),
        };
        Some(Reply {
            line: response_line(&p.rpc_id, result),
            ws_key: p.ws_key,
        })
    }

    /// `interaction/browserExecute`. `Some` is an immediate answer; `None`
    /// means the command went to the extension and a `Reply` follows.
    pub(crate) fn execute(
        &mut self,
        ws_key: &str,
        rpc_id: &Value,
        params: &Value,
        now: Instant,
    ) -> Option<String> {
        let fail = |code: &str, message: &str, side_effect: Option<&str>| {
            Some(response_line(
                rpc_id,
                failure(code, message, side_effect, Duration::ZERO),
            ))
        };
        let command = &params["command"];
        let (Some(request_id), Some(_)) =
            (params["requestId"].as_str(), command["method"].as_str())
        else {
            return fail("execution_error", "malformed browser request", Some("none"));
        };
        let requested = params["browserId"].as_str();
        let target = self
            .conns
            .iter()
            .filter_map(|(c, x)| x.browser.as_ref().map(|b| (*c, b)))
            .filter(|(_, b)| requested.is_none_or(|id| id == b.id))
            .max_by_key(|(_, b)| b.connected_at)
            .map(|(c, b)| (c, b.generation, b.id.clone()));
        let Some((conn, generation, id)) = target else {
            return fail(
                "backend_unavailable",
                &match requested {
                    Some(id) => format!("browser backend '{id}' is no longer available"),
                    None => "no browser is connected to ZCode".to_string(),
                },
                None,
            );
        };
        if let Some(asked) = params["browserGeneration"].as_u64()
            && asked != generation
        {
            return fail(
                "backend_unavailable",
                &format!("browser backend '{id}' generation {asked} is stale"),
                None,
            );
        }
        if self.pending.contains_key(request_id) {
            return fail(
                "duplicate_request_id",
                &format!("browser requestId '{request_id}' is already running"),
                Some("none"),
            );
        }
        let mut context = params.as_object().cloned().unwrap_or_default();
        for key in ["requestId", "command", "browserId", "browserGeneration"] {
            context.remove(key);
        }
        let message = json!({
            "op": "execute",
            "requestId": request_id,
            "command": command,
            "context": context,
        });
        if !self
            .conns
            .get(&conn)
            .is_some_and(|c| c.outbound.send(message.to_string()))
        {
            return fail(
                "backend_unavailable",
                "the browser extension is not reachable",
                None,
            );
        }
        self.pending.insert(
            request_id.to_string(),
            Pending {
                conn,
                ws_key: ws_key.to_string(),
                rpc_id: rpc_id.clone(),
                deadline: now + timeout_for(command),
                started: now,
                method: command["method"].as_str().unwrap_or("").to_string(),
                side_effects: may_have_side_effects(command),
            },
        );
        None
    }

    /// Ask the extension to stop `request_id`; its answer is not awaited.
    fn send_cancel(&self, conn: u64, request_id: &str) {
        if let Some(c) = self.conns.get(&conn) {
            let message = json!({
                "op": "execute",
                "requestId": format!("cancel:{request_id}"),
                "command": { "method": "cancelRequest", "requestId": request_id },
                "context": {},
            });
            c.outbound.send(message.to_string());
        }
    }

    /// Time out overdue requests (cancelling them in the browser).
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<Reply> {
        let overdue: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        let mut replies = Vec::new();
        for id in overdue {
            let Some(p) = self.pending.remove(&id) else {
                continue;
            };
            self.send_cancel(p.conn, &id);
            let message = format!(
                "browser command {} timed out ({}ms)",
                p.method,
                (p.deadline - p.started).as_millis()
            );
            replies.push(Reply {
                line: response_line(
                    &p.rpc_id,
                    failure(
                        "timeout",
                        &message,
                        Some(if p.side_effects { "uncertain" } else { "none" }),
                        now - p.started,
                    ),
                ),
                ws_key: p.ws_key,
            });
        }
        replies
    }

    /// The workspace's agent is gone: nobody waits for these answers.
    pub(crate) fn drop_workspace(&mut self, ws_key: &str) {
        let owned: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.ws_key == ws_key)
            .map(|(id, _)| id.clone())
            .collect();
        for id in owned {
            if let Some(p) = self.pending.remove(&id) {
                self.send_cancel(p.conn, &id);
            }
        }
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|p| p.deadline).min()
    }

    /// Stop listening and drop every socket, failing what was in flight.
    pub(crate) fn shutdown(&mut self, now: Instant) -> Vec<Reply> {
        self.listener = None;
        let conns: Vec<u64> = self.conns.keys().copied().collect();
        conns
            .into_iter()
            .flat_map(|c| self.closed(c, now))
            .collect()
    }
}
