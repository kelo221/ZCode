//! Reverse RPC dispatch for incoming agent-to-client requests on the stdio wire.
//!
//! Unlike client-to-agent commands (where the frontend is the client), the backend
//! agent also makes reverse JSON-RPC requests to the host/client during execution
//! (e.g. runtime preferences, provider runtime auth headers, automations, browser).
//!
//! Each method in the protocol must be explicitly classified:
//!   - `Raced`: Handled via V4 topics (e.g. `interaction/requestPermission`), ignored on stdio.
//!   - `Served`: Handled directly by this client with a schema-compliant response.
//!   - `FastFail`: Deliberately failed fast with a clear, actionable reason.
//!   - `Unknown`: Unrecognized method returning JSON-RPC -32601.
//!
//! Response shapes mirror the strict zod schemas in
//! `packages/shared/src/zcode-protocol/index.ts`; the wire format mirrors the
//! desktop host (`zcodeProtocolClient.respond{,Error}`: `{id, result|error}`).

use serde_json::{Value, json};

pub enum ReverseRpcAction {
    /// Raced with V4 topic (e.g. `pendingInteractions` + `resolveInteraction`).
    /// Do not answer via stdio; let V4 resolution win.
    Raced,
    /// Send this JSON line back to the backend.
    Respond(String),
}

/// JSON-RPC internal error: the host understood the request but cannot serve it.
const INTERNAL_ERROR: i64 = -32603;
const METHOD_NOT_FOUND: i64 = -32601;

/// Automations and off-peak tasks are scheduled by the desktop host
/// (`desktopCronScheduler.ts`), whose store GPUI cannot see. Answering with
/// empty lists or `bound: false` would be a false fact: the CLI treats
/// `automation/checkTaskBinding` as an authorization boundary that must fail
/// closed when unknown, and `-32601` would trigger its legacy-list fallback.
const SCHEDULER_UNAVAILABLE: &str = "Automations and off-peak tasks are managed by the ZCode desktop app; \
     they are not available in the GPUI client";

fn respond_result(id: &Value, result: Value) -> ReverseRpcAction {
    ReverseRpcAction::Respond(json!({ "id": id, "result": result }).to_string())
}

fn respond_error(id: &Value, code: i64, message: &str) -> ReverseRpcAction {
    ReverseRpcAction::Respond(
        json!({ "id": id, "error": { "code": code, "message": message } }).to_string(),
    )
}

/// Desktop-only method families that must fail fast with `-32603`, including
/// members added upstream after this table was written.
fn is_host_scheduler_method(method: &str) -> bool {
    method.starts_with("automation/") || method.starts_with("offPeak/")
}

/// Dispatches an agent-to-client request based on the method name.
pub fn dispatch_reverse_rpc(
    id: &Value,
    method: &str,
    _params: &Value,
    memory_enabled: bool,
) -> ReverseRpcAction {
    match method {
        "interaction/requestPermission" | "interaction/requestUserInput" => ReverseRpcAction::Raced,

        "interaction/requestProviderRuntimeHeaders" => respond_result(
            id,
            json!({
                "headersApplied": false,
                "errorMessage": "Zai account providers are not supported yet; configure an API-key provider"
            }),
        ),

        "interaction/requestOfficialMcpAuthHeaders" => respond_result(
            id,
            json!({ "ok": false, "reason": "official_auth_unavailable" }),
        ),

        "session/requestRuntimePreferences" => respond_result(
            id,
            json!({
                "askUserQuestionAutoResolutionEnabled": true,
                "nativeSearchEnhancementsEnabled": true,
                "memoryEnabled": memory_enabled
            }),
        ),

        // No embedded browser: discovery reports none, execution fails with
        // the schema's `backend_unavailable` code instead of hanging.
        "interaction/browserList" => respond_result(id, json!({ "browsers": [] })),
        "interaction/browserExecute" => respond_result(
            id,
            json!({
                "ok": false,
                "error": {
                    "code": "backend_unavailable",
                    "message": "browser control is not available in the GPUI client"
                },
                "elapsedMs": 0
            }),
        ),

        m if is_host_scheduler_method(m) => {
            respond_error(id, INTERNAL_ERROR, SCHEDULER_UNAVAILABLE)
        }

        other => respond_error(
            id,
            METHOD_NOT_FOUND,
            &format!("Unsupported ZCode Protocol request: {other}"),
        ),
    }
}

#[cfg(test)]
#[path = "reverse_rpc_tests.rs"]
mod tests;
