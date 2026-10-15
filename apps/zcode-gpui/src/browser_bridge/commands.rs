//! Pure helpers for the bridge state: result shapes, per-command timeouts and
//! side-effect classification, and sanitizing extension-supplied names.

use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Same budget as Desktop's host bridge (`browserControlMainBridge.ts`).
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Fixed waits get their own duration plus transport slack, otherwise a
/// 30-second wait would always time out here first.
const WAIT_SLACK: Duration = Duration::from_secs(2);

pub(crate) fn response_line(rpc_id: &Value, result: Value) -> String {
    json!({ "id": rpc_id, "result": result }).to_string()
}

pub(super) fn failure(
    code: &str,
    message: &str,
    side_effect: Option<&str>,
    elapsed: Duration,
) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(side_effect) = side_effect {
        error["sideEffect"] = json!(side_effect);
    }
    json!({ "ok": false, "error": error, "elapsedMs": elapsed.as_millis() as u64 })
}

pub(super) fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Mirrors Desktop's `mayHaveSideEffects`: whether a lost or timed-out
/// command may already have changed the page.
pub(super) fn may_have_side_effects(command: &Value) -> bool {
    let method = command["method"].as_str().unwrap_or("");
    if method == "playwright" && command["action"]["name"] == "locator" {
        let op = command["action"]["operation"].as_str().unwrap_or("");
        return matches!(
            op,
            "click"
                | "dblclick"
                | "downloadMedia"
                | "fill"
                | "press"
                | "selectOption"
                | "setChecked"
        );
    }
    matches!(
        method,
        "navigate"
            | "back"
            | "forward"
            | "reload"
            | "click"
            | "fill"
            | "type"
            | "press"
            | "cuaKeypress"
            | "scroll"
            | "cuaScroll"
            | "domCuaScroll"
            | "hover"
            | "select"
            | "check"
            | "drag"
            | "cuaDrag"
            | "recordingStart"
            | "recordingCancel"
            | "handleDialog"
            | "close"
            | "finalize"
            | "newTab"
    )
}

pub(super) fn timeout_for(command: &Value) -> Duration {
    let millis = |v: &Value| v.as_u64().map(Duration::from_millis);
    match command["method"].as_str() {
        Some("playwrightWaitForTimeout") => {
            millis(&command["timeoutMs"]).unwrap_or(DEFAULT_TIMEOUT) + WAIT_SLACK
        }
        Some("playwright") => {
            millis(&command["action"]["timeoutMs"]).unwrap_or(DEFAULT_TIMEOUT) + WAIT_SLACK
        }
        _ => DEFAULT_TIMEOUT,
    }
}

/// Profile ids come from the extension; keep descriptor ids printable.
pub(super) fn browser_id(profile_id: &str) -> Option<String> {
    let clean: String = profile_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(64)
        .collect();
    (!clean.is_empty()).then(|| format!("extension:{clean}"))
}

pub(super) fn text(value: &Value, fallback: &str) -> String {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(120).collect())
        .unwrap_or_else(|| fallback.to_string())
}
