//! `control` region of the V4 conversation state (snapshot.ts
//! `sessionControlStateSchema`): phase, stop availability, last error,
//! automatic retry and the active foreground execution.

use crate::conversation::model::ConversationState;
use serde_json::Value;

impl ConversationState {
    pub(crate) fn apply_control(&mut self, control: &Value) {
        self.can_stop = control.get("canStop").and_then(Value::as_bool) == Some(true);
        self.phase = control
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        self.last_error = match control.get("lastError") {
            Some(e) if !e.is_null() => Some((
                e.get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
                    .to_string(),
                // Provider errors may quote the rejected key verbatim.
                crate::shared::redact::scrub(
                    e.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error"),
                ),
            )),
            _ => None,
        };
        self.api_retry = match control.get("apiRetry") {
            Some(r) if !r.is_null() => Some(r.clone()),
            _ => None,
        };
        self.active_foreground_execution_id = control
            .get("activeWorks")
            .and_then(Value::as_array)
            .and_then(|works| {
                works.iter().find_map(|w| {
                    w.get("foregroundExecutionId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            });
    }
}
