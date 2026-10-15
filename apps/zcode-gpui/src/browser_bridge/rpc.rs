//! `AppState` glue: starts the bridge, applies socket events, answers the
//! agent's browser reverse RPCs and times out overdue requests.
//!
//! Event order for one command: agent `browserExecute` (stdio) ->
//! `BrowserBridge::execute` records it -> extension runs it -> `result` line
//! -> the pending entry is cleared -> `{id, result}` goes back on that
//! workspace's stdio. Timeouts and disconnects answer in its place.

use super::commands::response_line;
use super::listener::{self, BridgeEvent};
use super::state::Reply;
use crate::app::store::AppState;
use futures::StreamExt;
use gpui::Context;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(250);

impl AppState {
    /// Desktop startup. A scratch (isolated) instance must not touch the
    /// real home, where the host looks for the endpoint file.
    pub(crate) fn init_browser_bridge(&mut self, cx: &mut Context<Self>) {
        if crate::shared::isolation::active().is_some() || self.browser.listener.is_some() {
            return;
        }
        let Some(path) = super::native::endpoint_path() else {
            self.browser.error = Some("no home directory for the browser bridge".into());
            return;
        };
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        match listener::start(path, tx) {
            Ok(handle) => {
                self.push_log(format!("browser bridge listening on {}", handle.addr));
                self.browser.listener = Some(handle);
                self.browser.error = None;
            }
            Err(error) => {
                self.browser.error = Some(error);
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                let alive = this
                    .update(cx, |state, cx| state.handle_bridge_event(event, cx))
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this
                    .update(cx, |state, _| {
                        if state.browser.listener.is_none() {
                            return false;
                        }
                        if state
                            .browser
                            .next_deadline()
                            .is_some_and(|d| d <= Instant::now())
                        {
                            let replies = state.browser.expire(Instant::now());
                            state.deliver_browser_replies(replies);
                        }
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

    fn handle_bridge_event(&mut self, event: BridgeEvent, cx: &mut Context<Self>) {
        let now = Instant::now();
        let replies = match event {
            BridgeEvent::Connected { conn, outbound } => {
                self.browser.connected(conn, outbound);
                Vec::new()
            }
            BridgeEvent::Line { conn, line } => {
                let before = self.browser.browsers().len();
                let replies = self.browser.handle_line(conn, &line, now);
                if self.browser.browsers().len() != before {
                    self.push_log("browser extension connected".into());
                }
                replies
            }
            BridgeEvent::Closed { conn } => self.browser.closed(conn, now),
        };
        self.deliver_browser_replies(replies);
        cx.notify();
    }

    fn deliver_browser_replies(&mut self, replies: Vec<Reply>) {
        for reply in replies {
            if let Some(ws) = self.ws_mut(&reply.ws_key) {
                ws.send_line(reply.line);
            }
        }
    }

    /// Answer `interaction/browserList|Execute` while the bridge listens.
    /// Returns false to fall back to `reverse_rpc` (no bridge: no browsers).
    pub(crate) fn route_browser_rpc(
        &mut self,
        ws_key: &str,
        id: &Value,
        method: &str,
        params: &Value,
    ) -> bool {
        if self.browser.listener.is_none() {
            return false;
        }
        let immediate = match method {
            "interaction/browserList" => Some(response_line(
                id,
                json!({ "browsers": self.browser.descriptors() }),
            )),
            "interaction/browserExecute" => {
                self.browser.execute(ws_key, id, params, Instant::now())
            }
            _ => return false,
        };
        if let Some(line) = immediate
            && let Some(ws) = self.ws_mut(ws_key)
        {
            ws.send_line(line);
        }
        true
    }
}
