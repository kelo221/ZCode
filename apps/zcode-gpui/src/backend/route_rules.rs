//! Pure route rules for V4 topic frames (split from backend/route.rs for the
//! 400-line cap): the continuity/identity decision and topic-specific payload
//! shape validation. The client half of
//! packages/shared/src/zcode-protocol-v4/controller.ts
//! `isWindowHostControllerFrameGap` + transport.ts frame rules.
//!
//! Invariants (follow-up review finding 1):
//! - The active route is `{subscriptionId, logEpoch}` from the latest
//!   subscribe/resync ACK; frames without an owning active route fail closed.
//! - A snapshot may establish a cursor only if its subscription id matches
//!   the active route and its payload `logEpoch` matches the ACK epoch.
//! - A delta applies only if the cursor generation equals the active route
//!   generation, the frame subscription matches, and `fromSeq == cursor.seq`.

use serde_json::Value;

/// What the route layer decided to do with a frame. Pure function — golden
/// tested in route_rules_tests.rs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FrameDecision {
    /// Apply the payload, then advance the cursor to `toSeq`.
    Apply,
    /// Stale / duplicate / old-generation frame: drop silently.
    Drop,
    /// Discontinuity, no base, malformed range, or epoch mismatch: resync
    /// the route and do not touch any mirrored state.
    Resync,
}

/// `cursor` is `(subscriptionId, logEpoch, seq)` of the last applied frame on
/// the topic; `active` is `(subscriptionId, logEpoch)` of the live route from
/// the latest subscribe/resync ACK; `snapshot_epoch` is the logEpoch carried
/// in a snapshot payload, if any.
pub(crate) fn classify_frame(
    cursor: Option<(&str, &str, u64)>,
    active: Option<(&str, &str)>,
    kind: &str,
    frame_sub: &str,
    from_seq: u64,
    to_seq: u64,
    snapshot_epoch: Option<&str>,
) -> FrameDecision {
    if to_seq < from_seq {
        return FrameDecision::Resync;
    }
    // Fail closed: without an owning active route nothing may apply (the
    // transport contract delivers the subscribe ack before the first frame).
    let Some((active_sub, active_epoch)) = active else {
        return FrameDecision::Drop;
    };
    match kind {
        "snapshot" => {
            if active_sub != frame_sub {
                // An old generation's snapshot would roll live state back.
                return FrameDecision::Drop;
            }
            // A recovery snapshot whose epoch disagrees with the latest ACK
            // belongs to a different log generation.
            if !active_epoch.is_empty()
                && let Some(epoch) = snapshot_epoch
                && epoch != active_epoch
            {
                return FrameDecision::Resync;
            }
            // Canonical frame schema: snapshots start at seq zero.
            if from_seq != 0 {
                return FrameDecision::Resync;
            }
            FrameDecision::Apply
        }
        "deltas" => {
            match cursor {
                Some((cursor_sub, cursor_epoch, seq)) if cursor_sub == frame_sub => {
                    // The cursor generation must still be the ACTIVE route
                    // generation: a contiguous delta from a superseded
                    // subscription (e.g. after reconnect) is a leftover.
                    if cursor_sub != active_sub
                        || (!active_epoch.is_empty() && cursor_epoch != active_epoch)
                    {
                        return FrameDecision::Drop;
                    }
                    if to_seq <= seq {
                        // Duplicate or fully-superseded frame: never applied.
                        FrameDecision::Drop
                    } else if from_seq != seq {
                        // Gap (including the one-event gap
                        // `fromSeq == seq + 1`) and partial overlap alike:
                        // the baseline is untrustworthy.
                        FrameDecision::Resync
                    } else {
                        FrameDecision::Apply
                    }
                }
                _ => {
                    // Deltas from a generation we never applied: if it is the
                    // active subscription we missed its snapshot → resync;
                    // otherwise it is an old-generation leftover → drop.
                    if frame_sub == active_sub {
                        FrameDecision::Resync
                    } else {
                        FrameDecision::Drop
                    }
                }
            }
        }
        // Unknown payload kinds are tolerated (never applied): PARITY.md §6.
        _ => FrameDecision::Drop,
    }
}

/// Topic-specific payload shape validation (review finding 3): a known
/// payload whose required fields are missing or wrongly typed is a fault,
/// never a silent no-op that would advance the cursor past an unapplied
/// operation. Unknown operations inside a valid `deltas` array stay
/// tolerated (forward compatibility, PARITY.md §6).
pub(crate) fn validate_payload_shape(
    topic: &str,
    kind: &str,
    payload: &Value,
) -> Result<(), String> {
    let obj = payload
        .as_object()
        .ok_or_else(|| "payload is not an object".to_string())?;
    if kind == "snapshot" {
        let snap = obj
            .get("snapshot")
            .ok_or_else(|| "snapshot payload missing 'snapshot'".to_string())?;
        if !snap.is_object() {
            return Err("snapshot is not an object".into());
        }
        if topic.starts_with("sessions-index/")
            && !snap.get("sessions").is_some_and(Value::is_array)
        {
            return Err("sessions-index snapshot missing sessions array".into());
        }
        if topic.starts_with("workspace-config/")
            && !snap.get("config").is_some_and(Value::is_object)
        {
            return Err("workspace-config snapshot missing config object".into());
        }
        Ok(())
    } else if kind == "deltas" {
        if obj.get("deltas").is_some_and(Value::is_array) {
            Ok(())
        } else {
            Err("deltas payload missing deltas array".into())
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "route_rules_tests.rs"]
mod tests;
