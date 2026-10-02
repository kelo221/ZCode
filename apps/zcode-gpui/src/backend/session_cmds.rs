//! Session-scoped V4 commands: one envelope builder, CAS fields filled from
//! the mirrored conversation, and ack handling (stale retry, failure rollback).
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts
//! (`commandEnvelopeSchema`, `COMMANDS_REQUIRING_BASE_REVISION`,
//! `ROW_TARGETING_COMMANDS`, `commandAckSchema`).
//!
//! Ownership: the backend is the only source of truth. Optimistic UI updates
//! are rolled back by re-fetching the authoritative snapshot (forced resync)
//! instead of keeping a second, client-side undo path.

use crate::app::store::AppState;
use crate::backend::workspace::{CommandCtx, Pending};
use serde_json::{Value, json};

/// `v4/command` params (the CommandEnvelope). Pure so golden tests can pin
/// the wire shape of every command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn command_params(
    client_id: &str,
    sid: Option<&str>,
    ctype: &str,
    payload: Value,
    base_revision: Option<u64>,
    base_log_epoch: Option<&str>,
    command_id: String,
    issued_at: u64,
) -> Value {
    let mut params = json!({
        "commandId": command_id,
        "clientId": client_id,
        "sessionId": sid,
        "type": ctype,
        "payload": payload,
        "issuedAt": issued_at,
    });
    if let Some(base) = base_revision {
        params["baseRevision"] = json!(base);
    }
    if let Some(epoch) = base_log_epoch {
        params["baseLogEpoch"] = json!(epoch);
    }
    params
}

/// What to do with a command ack (`commandAckSchema.status`).
#[derive(Debug, PartialEq)]
pub(crate) enum AckAction {
    /// accepted / noop / duplicate.
    Settled,
    /// First `stale` of a CAS command: resend from `revisionAtDecision`.
    Retry,
    /// rejected / failed / repeated stale: report and roll back.
    Fail,
}

pub(crate) fn ack_action(status: &str, cas: bool, retried: bool) -> AckAction {
    match status {
        "accepted" | "noop" | "duplicate" => AckAction::Settled,
        "stale" if cas && !retried => AckAction::Retry,
        _ => AckAction::Fail,
    }
}

/// Row-targeting payload: `{ target: { rowId, entityId }, ...extra }`.
pub(crate) fn row_payload(row_id: u64, entity_id: &str, extra: Value) -> Value {
    let mut payload = json!({ "target": { "rowId": row_id, "entityId": entity_id } });
    if let (Some(obj), Value::Object(more)) = (payload.as_object_mut(), extra) {
        obj.extend(more);
    }
    payload
}

/// Human label for error banners.
fn command_label(ctype: &str) -> &str {
    match ctype {
        "editUserQuery" => "Edit message",
        "retryTurn" => "Retry",
        "applyFileRewind" => "Undo file changes",
        "renameSession" => "Rename session",
        "deleteSession" => "Delete session",
        "sendQueuedNow" => "Send queued message",
        "deleteQueueItem" => "Remove queued message",
        "setAutoDrain" => "Pause/resume queue",
        "resolveInteraction" => "Answer",
        "switchModelConfig" => "Model switch",
        "switchCollaborationMode" => "Mode switch",
        other => other,
    }
}

impl AppState {
    /// Send a session command. CAS commands take `baseRevision` from the
    /// mirrored conversation; there is no fallback revision, because a guessed
    /// base can only ever come back `stale`.
    pub(crate) fn send_session_command(&mut self, ws_key: &str, ctx: CommandCtx) -> bool {
        let base_revision = if ctx.cas {
            match self.conversations.get(&ctx.sid) {
                Some(c) => Some(c.revision),
                None => {
                    self.push_error(format!(
                        "{}: conversation is still loading",
                        command_label(&ctx.ctype)
                    ));
                    return false;
                }
            }
        } else {
            None
        };
        let params = command_params(
            &self.client_id,
            Some(&ctx.sid),
            &ctx.ctype,
            ctx.payload.clone(),
            base_revision,
            ctx.log_epoch.as_deref(),
            crate::backend::launcher::new_command_id(),
            crate::backend::launcher::now_ms(),
        );
        self.send_envelope(ws_key, params, Pending::Command(ctx))
    }

    /// Row-targeting commands need the snapshot's `logEpoch`; without it the
    /// action is unavailable rather than sent with a guessed epoch.
    pub(crate) fn row_epoch(&mut self, sid: &str) -> Option<String> {
        let epoch = self
            .conversations
            .get(sid)
            .and_then(|c| c.log_epoch.clone());
        if epoch.is_none() {
            self.push_error("Conversation is still loading; try again in a moment".into());
        }
        epoch
    }

    pub(crate) fn handle_command_ack(
        &mut self,
        ws_key: &str,
        ack: Option<&Value>,
        ctx: CommandCtx,
    ) {
        let field = |k: &str| ack.and_then(|a| a.get(k)).and_then(Value::as_str);
        let status = field("status").unwrap_or("accepted").to_string();
        let revision = ack
            .and_then(|a| a.get("revisionAtDecision"))
            .and_then(Value::as_u64);
        match ack_action(&status, ctx.cas, ctx.retried) {
            AckAction::Settled => {
                // An accepted command bumps the revision past the decision point.
                let next = revision.map(|r| if status == "accepted" { r + 1 } else { r });
                if let (Some(next), Some(c)) = (next, self.conversations.get_mut(&ctx.sid)) {
                    c.revision = c.revision.max(next);
                }
                self.check_result(ack.and_then(|a| a.get("result")), &ctx);
            }
            // Revision drifted (internal backend events bump it between
            // snapshots). Retry once from the server's decision revision with a
            // new commandId; the log epoch still guards row identity.
            AckAction::Retry => {
                if let (Some(r), Some(c)) = (revision, self.conversations.get_mut(&ctx.sid)) {
                    c.revision = r;
                }
                let retry = CommandCtx {
                    retried: true,
                    ..ctx
                };
                self.send_session_command(ws_key, retry);
            }
            AckAction::Fail => {
                let reason = field("message").or(field("reasonCode")).unwrap_or("");
                self.push_error(format!(
                    "{} {status}{}{reason}",
                    command_label(&ctx.ctype),
                    if reason.is_empty() { "" } else { ": " }
                ));
                self.rollback_command(ws_key, &ctx);
            }
        }
    }

    /// Accepted acks can still report that nothing happened.
    fn check_result(&mut self, result: Option<&Value>, ctx: &CommandCtx) {
        let Some(result) = result else { return };
        let label = command_label(&ctx.ctype);
        if ctx.ctype == "applyFileRewind"
            && result.get("applied").and_then(Value::as_bool) == Some(false)
        {
            let why = result.get("response").and_then(Value::as_str).unwrap_or("");
            self.push_error(format!("{label} not applied: {why}"));
        }
        if ctx.ctype == "editUserQuery"
            && result.get("disposition").and_then(Value::as_str) == Some("blocked")
        {
            let why = result
                .get("reasonCode")
                .and_then(Value::as_str)
                .unwrap_or("blocked");
            self.push_error(format!("{label} blocked: {why}"));
        }
    }

    /// Re-fetch the authoritative state an optimistic update diverged from.
    pub(crate) fn rollback_command(&mut self, ws_key: &str, ctx: &CommandCtx) {
        let topic = match ctx.ctype.as_str() {
            "deleteSession" | "renameSession" => format!("sessions-index/{ws_key}"),
            _ => format!("conversation/{}", ctx.sid),
        };
        self.resync_topic(ws_key, &topic);
    }
}

#[cfg(test)]
#[path = "session_cmds_tests.rs"]
mod tests;
