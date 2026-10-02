//! Golden tests for `session_cmds.rs`: envelope shapes per the V4
//! `parseCommandEnvelope` rules and ack handling decisions.

use super::*;
use crate::workspace::CommandCtx;

fn envelope(ctx: &CommandCtx, base: Option<u64>) -> Value {
    command_params(
        "client-1",
        Some(&ctx.sid),
        &ctx.ctype,
        ctx.payload.clone(),
        base,
        ctx.log_epoch.as_deref(),
        "cmd-1".into(),
        42,
    )
}

#[test]
fn row_commands_carry_base_revision_and_log_epoch() {
    for ctype in ["retryTurn", "applyFileRewind"] {
        let ctx = CommandCtx::new("s1", ctype, row_payload(7, "e7", json!({}))).row("ep-1".into());
        let env = envelope(&ctx, Some(12));
        assert_eq!(
            env,
            json!({
                "commandId": "cmd-1", "clientId": "client-1", "sessionId": "s1",
                "type": ctype, "issuedAt": 42,
                "baseRevision": 12, "baseLogEpoch": "ep-1",
                // conversationRowTargetSchema is strict: exactly rowId + entityId.
                "payload": { "target": { "rowId": 7, "entityId": "e7" } },
            })
        );
    }
}

#[test]
fn edit_user_query_payload() {
    let extra = json!({ "newText": "fixed", "workspaceMode": "preserve" });
    let ctx = CommandCtx::new("s1", "editUserQuery", row_payload(3, "e3", extra)).row("ep".into());
    let env = envelope(&ctx, Some(1));
    assert_eq!(
        env["payload"],
        json!({ "target": { "rowId": 3, "entityId": "e3" },
                "newText": "fixed", "workspaceMode": "preserve" })
    );
    assert_eq!(env["baseLogEpoch"], "ep");
}

#[test]
fn cas_commands_carry_base_revision_only() {
    for (ctype, payload) in [
        ("sendQueuedNow", json!({ "queueItemId": "q1" })),
        ("deleteQueueItem", json!({ "queueItemId": "q1" })),
        ("setAutoDrain", json!({ "autoDrain": false })),
        (
            "switchModelConfig",
            json!({ "provider": "p", "model": "m", "thought": "" }),
        ),
        ("switchCollaborationMode", json!({ "mode": "yolo" })),
    ] {
        let ctx = CommandCtx::new("s1", ctype, payload).cas();
        assert!(ctx.cas && ctx.log_epoch.is_none());
        let env = envelope(&ctx, Some(5));
        assert_eq!(env["baseRevision"], 5);
        assert!(env.get("baseLogEpoch").is_none());
    }
}

#[test]
fn non_cas_commands_omit_cas_fields() {
    for (ctype, payload) in [
        ("renameSession", json!({ "title": "t" })),
        ("deleteSession", json!({})),
        (
            "resolveInteraction",
            json!({ "interactionId": "i1", "answer": { "optionId": "allowOnce" } }),
        ),
    ] {
        let ctx = CommandCtx::new("s1", ctype, payload);
        assert!(!ctx.cas);
        let env = envelope(&ctx, None);
        assert!(env.get("baseRevision").is_none());
        assert!(env.get("baseLogEpoch").is_none());
    }
}

#[test]
fn create_session_envelope_has_null_session() {
    let env = command_params(
        "c",
        None,
        "createSession",
        json!({}),
        None,
        None,
        "x".into(),
        0,
    );
    assert!(env["sessionId"].is_null());
}

#[test]
fn ack_decisions() {
    assert_eq!(ack_action("accepted", true, false), AckAction::Settled);
    assert_eq!(ack_action("noop", false, false), AckAction::Settled);
    assert_eq!(ack_action("duplicate", true, true), AckAction::Settled);
    // stale retries exactly once, and only for CAS commands.
    assert_eq!(ack_action("stale", true, false), AckAction::Retry);
    assert_eq!(ack_action("stale", true, true), AckAction::Fail);
    assert_eq!(ack_action("stale", false, false), AckAction::Fail);
    assert_eq!(ack_action("rejected", true, false), AckAction::Fail);
    assert_eq!(ack_action("failed", false, false), AckAction::Fail);
}
