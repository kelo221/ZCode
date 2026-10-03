//! Golden tests for `backend/session_cmds.rs`: envelope shapes per the V4
//! `parseCommandEnvelope` rules and ack handling decisions.

use super::*;
use crate::backend::workspace::CommandCtx;

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
    for ctype in [
        "retryTurn",
        "applyFileRewind",
        "forkAssistant",
        "setAssistantFeedback",
    ] {
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
        (
            "editQueueItem",
            json!({ "queueItemId": "q1", "newText": "hi" }),
        ),
        (
            "reorderQueueItem",
            json!({ "queueItemId": "q1", "beforeQueueItemId": null }),
        ),
        ("deleteQueueItem", json!({ "queueItemId": "q1" })),
        ("setAutoDrain", json!({ "autoDrain": false })),
        (
            "switchModelConfig",
            json!({ "provider": "p", "model": "m", "thought": "" }),
        ),
        ("switchCollaborationMode", json!({ "mode": "yolo" })),
        ("pauseGoal", json!({})),
        ("resumeGoal", json!({})),
        ("setFollowupMode", json!({ "mode": "guide" })),
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
        ("compact", json!({})),
        ("sendGoalCommand", json!({ "text": "objective" })),
        ("cancelBackgroundWork", json!({ "workId": "w1" })),
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

#[test]
fn send_text_with_attachments_payload() {
    let att = crate::composer::attachment::AttachmentRef {
        reference: "C:\\path\\photo.png".into(),
        file_name: "photo.png".into(),
        mime: "image/png".into(),
        bytes: 1024,
        preview_ref: None,
    };
    let payload = json!({
        "text": "Check this",
        "attachments": [att],
    });
    let ctx = CommandCtx::new("s1", "sendText", payload);
    let env = envelope(&ctx, None);
    assert_eq!(env["payload"]["attachments"][0]["fileName"], "photo.png");
    assert_eq!(
        env["payload"]["attachments"][0]["ref"],
        "C:\\path\\photo.png"
    );
}

#[test]
fn spec_command_lists_match_command_ts() {
    // Every row-targeting command is also a CAS command (command.ts).
    for ctype in ROW_TARGETING_COMMANDS {
        assert!(COMMANDS_REQUIRING_BASE_REVISION.contains(ctype), "{ctype}");
    }
    assert_eq!(COMMANDS_REQUIRING_BASE_REVISION.len(), 15);
    assert_eq!(ROW_TARGETING_COMMANDS.len(), 5);
}

#[test]
fn cas_fields_take_base_strictly_from_mirror() {
    assert_eq!(
        cas_fields("switchModelConfig", true, None, Some(9), None),
        Ok((Some(9), None))
    );
    // No mirrored snapshot revision: blocked, never defaulted to 0.
    assert_eq!(
        cas_fields("switchModelConfig", true, None, None, None),
        Err(CasBlock::NoRevision)
    );
}

#[test]
fn cas_fields_enforce_spec_lists_even_if_caller_forgot_cas() {
    // A spec CAS command built without `.cas()` still carries baseRevision.
    assert_eq!(
        cas_fields("deleteQueueItem", false, None, Some(4), None),
        Ok((Some(4), None))
    );
    assert_eq!(
        cas_fields("pauseGoal", false, None, None, None),
        Err(CasBlock::NoRevision)
    );
    // Non-CAS commands never carry CAS fields, even when the mirror has them.
    assert_eq!(
        cas_fields("compact", false, None, Some(4), Some("ep")),
        Ok((None, None))
    );
}

#[test]
fn row_commands_without_epoch_are_rejected_client_side() {
    for ctype in ROW_TARGETING_COMMANDS {
        assert_eq!(
            cas_fields(ctype, true, None, Some(3), None),
            Err(CasBlock::NoLogEpoch),
            "{ctype}"
        );
        assert_eq!(
            cas_fields(ctype, true, None, Some(3), Some("  ")),
            Err(CasBlock::NoLogEpoch),
            "{ctype}: blank epoch"
        );
        assert_eq!(
            cas_fields(ctype, false, None, Some(3), Some("ep-1")),
            Ok((Some(3), Some("ep-1")))
        );
    }
}

#[test]
fn stale_retry_uses_revision_at_decision_not_mirror() {
    // Retry base wins over the (older) mirror and works before a snapshot.
    assert_eq!(
        cas_fields("retryTurn", true, Some(21), Some(17), Some("ep")),
        Ok((Some(21), Some("ep")))
    );
    assert_eq!(
        cas_fields("setAutoDrain", true, Some(21), None, None),
        Ok((Some(21), None))
    );
}

#[test]
fn stale_retry_happens_at_most_once() {
    assert_eq!(ack_action("stale", true, false), AckAction::Retry);
    assert_eq!(ack_action("stale", true, true), AckAction::Fail);
    // Unknown/missing statuses are failures, never silently settled.
    assert_eq!(ack_action("malformed ack", true, false), AckAction::Fail);
    assert_eq!(ack_action("", false, false), AckAction::Fail);
}
