//! Golden tests for the pure route rules (backend/route_rules.rs): continuity
//! with active-generation/epoch enforcement (review finding 1) and payload
//! shape validation (finding 3).

use super::FrameDecision::*;
use super::classify_frame;
use super::validate_payload_shape;

const SUB_A: &str = "sub-a";
const SUB_B: &str = "sub-b";
const E1: &str = "epoch-1";
const E2: &str = "epoch-2";

#[test]
fn exact_continuity_is_accepted() {
    // (5, 7] accepted when the cursor is 5 and the cursor generation is the
    // active route generation.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            5,
            7,
            None
        ),
        Apply
    );
    // Empty delta range (5, 5] is a no-op that still confirms the cursor.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            5,
            5,
            None
        ),
        Drop
    );
}

#[test]
fn one_event_gap_and_overlap_are_rejected() {
    // (6, 7] with cursor at 5 has silently skipped event 6.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            6,
            7,
            None
        ),
        Resync
    );
    // (4, 7] overlaps the cursor: reapplying would double-mutate.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            4,
            7,
            None
        ),
        Resync
    );
}

#[test]
fn duplicates_are_dropped_without_resync() {
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            0,
            5,
            None
        ),
        Drop
    );
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "deltas",
            SUB_A,
            0,
            3,
            None
        ),
        Drop
    );
}

#[test]
fn late_contiguous_delta_from_superseded_generation_drops() {
    // Review case: cursor = A at seq 5, active route = B, a late contiguous
    // delta from A must NOT apply — the cursor generation is no longer
    // active.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_B, E1)),
            "deltas",
            SUB_A,
            5,
            7,
            None
        ),
        Drop
    );
    // Same when only the epoch moved (resync ACK with a new log generation):
    // deltas from the old epoch are dropped until the recovery snapshot
    // re-bases the cursor.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E2)),
            "deltas",
            SUB_A,
            5,
            7,
            None
        ),
        Drop
    );
}

#[test]
fn frames_without_an_active_route_fail_closed() {
    // No active route: nothing may apply — not snapshots, not deltas.
    assert_eq!(
        classify_frame(None, None, "snapshot", SUB_B, 0, 9, Some(E1)),
        Drop
    );
    assert_eq!(
        classify_frame(Some((SUB_A, E1, 5)), None, "deltas", SUB_A, 5, 7, None),
        Drop
    );
}

#[test]
fn snapshot_epoch_mismatch_is_a_fault() {
    // ACK says epoch-2, the recovery snapshot claims epoch-1: fault/resync,
    // never applied.
    assert_eq!(
        classify_frame(None, Some((SUB_A, E2)), "snapshot", SUB_A, 0, 9, Some(E1)),
        Resync
    );
    // Matching epoch applies and re-bases the cursor.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E2)),
            "snapshot",
            SUB_A,
            0,
            9,
            Some(E2)
        ),
        Apply
    );
    // An ACK without an epoch cannot enforce the check (older backend).
    assert_eq!(
        classify_frame(None, Some((SUB_A, "")), "snapshot", SUB_A, 0, 9, Some(E1)),
        Apply
    );
}

#[test]
fn resync_ack_then_recovery_snapshot_then_deltas() {
    // After a resync the active epoch moved to E2; the old cursor still
    // points at (A, E1). The recovery snapshot (epoch E2, from 0) applies…
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E2)),
            "snapshot",
            SUB_A,
            0,
            9,
            Some(E2)
        ),
        Apply
    );
    // …and the next contiguous delta applies against the new base.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E2, 9)),
            Some((SUB_A, E2)),
            "deltas",
            SUB_A,
            9,
            11,
            None
        ),
        Apply
    );
}

#[test]
fn old_generation_frames_and_unknown_kinds_never_apply() {
    // Deltas of the ACTIVE subscription whose snapshot we never applied.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_B, E1)),
            "deltas",
            SUB_B,
            0,
            2,
            None
        ),
        Resync
    );
    // Frames for a subscription we do not hold at all → drop.
    assert_eq!(
        classify_frame(None, Some((SUB_B, E1)), "deltas", SUB_A, 0, 2, None),
        Drop
    );
    // Old-generation snapshot must not roll live state back.
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "snapshot",
            SUB_A,
            3,
            9,
            None
        ),
        Resync
    );
    // Tolerated-but-unapplied payload kind (PARITY.md §6).
    assert_eq!(
        classify_frame(
            Some((SUB_A, E1, 5)),
            Some((SUB_A, E1)),
            "something-new",
            SUB_A,
            5,
            7,
            None
        ),
        Drop
    );
}

// --- Payload shape validation (finding 3) -----------------------------------

fn sessions_snapshot(sessions: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "kind": "snapshot", "snapshot": { "logEpoch": "e", "sessions": sessions } })
}

#[test]
fn valid_payload_shapes_pass() {
    assert_eq!(
        validate_payload_shape(
            "conversation/s",
            "snapshot",
            &sessions_snapshot(serde_json::json!([]))
        ),
        Ok(())
    );
    assert_eq!(
        validate_payload_shape(
            "conversation/s",
            "deltas",
            &serde_json::json!({ "kind": "deltas", "deltas": [{ "op": "row.delta", "unknownNewField": 1 }] })
        ),
        Ok(())
    );
    assert_eq!(
        validate_payload_shape(
            "workspace-config/w",
            "snapshot",
            &serde_json::json!({ "kind": "snapshot", "snapshot": { "config": { "mode": "build" } } })
        ),
        Ok(())
    );
}

#[test]
fn malformed_payloads_are_faults_not_noops() {
    // Missing snapshot object entirely.
    assert!(
        validate_payload_shape(
            "conversation/s",
            "snapshot",
            &serde_json::json!({ "kind": "snapshot" })
        )
        .is_err()
    );
    // Non-array deltas.
    assert!(
        validate_payload_shape(
            "conversation/s",
            "deltas",
            &serde_json::json!({ "kind": "deltas", "deltas": "oops" })
        )
        .is_err()
    );
    // A malformed sessions snapshot must not become an authoritative empty
    // list.
    assert!(validate_payload_shape(
        "sessions-index/w",
        "snapshot",
        &serde_json::json!({ "kind": "snapshot", "snapshot": { "logEpoch": "e", "sessions": 5 } })
    )
    .is_err());
    assert!(
        validate_payload_shape(
            "sessions-index/w",
            "snapshot",
            &serde_json::json!({ "kind": "snapshot", "snapshot": { "logEpoch": "e" } })
        )
        .is_err()
    );
    // Missing workspace-config state.
    assert!(
        validate_payload_shape(
            "workspace-config/w",
            "snapshot",
            &serde_json::json!({ "kind": "snapshot", "snapshot": { "logEpoch": "e" } })
        )
        .is_err()
    );
    // Payload not an object at all.
    assert!(validate_payload_shape("conversation/s", "deltas", &serde_json::json!(7)).is_err());
}
