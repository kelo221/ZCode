//! One-off read-only requests whose answer goes back to an async caller
//! instead of into app state (transcript image bytes: `remote::desktop_images`).
//! The pending entry owns the sender, so a dropped connection (pending
//! cleared) ends the wait with `Canceled` rather than leaving it hanging.

use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use futures::channel::oneshot;
use gpui::Context;
use serde_json::{Value, json};

pub(crate) type QueryReply = Result<Value, String>;

/// Shown when the desktop gateway does not know a GCode-only method.
pub(crate) const DESKTOP_TOO_OLD: &str = "Desktop GCode is too old to send this image";

impl AppState {
    pub(crate) fn raw_query(
        &mut self,
        ws_key: &str,
        method: &str,
        params: Value,
    ) -> oneshot::Receiver<QueryReply> {
        let (tx, rx) = oneshot::channel();
        let Some(ws) = self
            .ws_mut(ws_key)
            .filter(|ws| ws.started && ws.inbound.is_some())
        else {
            let _ = tx.send(Err("Not connected".into()));
            return rx;
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::RawQuery(tx));
        ws.send_pending_line(
            id,
            json!({"id": id, "method": method, "params": params}).to_string(),
        );
        rx
    }

    pub(crate) fn handle_raw_query_response(
        &mut self,
        ws_key: &str,
        id: u64,
        result: &Option<Value>,
        error: &Option<Value>,
        _cx: &mut Context<Self>,
    ) -> bool {
        let Some(ws) = self.ws_mut(ws_key) else {
            return false;
        };
        if !matches!(ws.pending.get(&id), Some(Pending::RawQuery(_))) {
            return false;
        }
        let Some(Pending::RawQuery(tx)) = ws.pending.remove(&id) else {
            return false;
        };
        let reply = match (error, result) {
            (Some(err), _) if crate::remote::scope::is_phone_scope_refusal(err) => {
                Err(DESKTOP_TOO_OLD.to_string())
            }
            (Some(err), _) => Err(err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("request failed")
                .to_string()),
            (None, Some(value)) => Ok(value.clone()),
            (None, None) => Err("empty response".into()),
        };
        let _ = tx.send(reply);
        true
    }
}
