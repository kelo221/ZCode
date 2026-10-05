//! Golden tests for the route continuity decision (backend/route.rs). These
//! are the acceptance cases from the 2026-10-05 parity audit (P0.1).

use super::FrameDecision::*;
use super::classify_frame;

const CUR_SUB: &str = "sub-a";
const NEW_SUB: &str = "sub-b";
const OLD_SUB: &str = "sub-old";

#[test]
fn exact_continuity_is_accepted() {
    // (5, 7] accepted when the cursor is 5.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 5, 7),
        Apply
    );
    // Empty delta range (5, 5] is a no-op that still confirms the cursor.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 5, 5),
        Drop
    );
}

#[test]
fn one_event_gap_is_rejected() {
    // (6, 7] with cursor at 5 has silently skipped event 6 — the old
    // `from_seq > last + 1` test accepted exactly this bug.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 6, 7),
        Resync
    );
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 7, 9),
        Resync
    );
}

#[test]
fn duplicates_and_full_overlap_are_dropped_without_resync() {
    // Frame ending at the cursor or earlier is stale.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 4, 5),
        Drop
    );
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 0, 3),
        Drop
    );
}

#[test]
fn partial_overlap_is_a_gap_not_a_reapply() {
    // (4, 7] with cursor at 5: events 4 were already counted, 6..7 were not;
    // reapplying would double-mutate, so resync instead.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 4, 7),
        Resync
    );
}

#[test]
fn old_generation_frames_are_rejected() {
    // Deltas from an old subscription when a newer cursor exists → drop.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", OLD_SUB, 5, 7),
        Drop
    );
    // Old-generation snapshot must not roll live state back.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "snapshot", OLD_SUB, 0, 9),
        Drop
    );
    // Deltas of the ACTIVE subscription whose snapshot we never applied.
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(NEW_SUB), "deltas", NEW_SUB, 0, 2),
        Resync
    );
    // No cursor yet (reconnect): deltas need a snapshot first.
    assert_eq!(
        classify_frame(None, Some(NEW_SUB), "deltas", NEW_SUB, 0, 2),
        Resync
    );
    // Frames for a subscription we do not hold at all → drop.
    assert_eq!(
        classify_frame(None, Some(NEW_SUB), "deltas", OLD_SUB, 0, 2),
        Drop
    );
}

#[test]
fn snapshots_reestablish_the_base() {
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "snapshot", CUR_SUB, 0, 9),
        Apply
    );
    assert_eq!(
        classify_frame(None, Some(NEW_SUB), "snapshot", NEW_SUB, 0, 0),
        Apply
    );
    // Malformed snapshot: canonical schema requires fromSeq == 0.
    assert_eq!(
        classify_frame(None, Some(NEW_SUB), "snapshot", NEW_SUB, 3, 9),
        Resync
    );
}

#[test]
fn malformed_ranges_and_unknown_kinds_never_apply() {
    assert_eq!(
        classify_frame(Some((CUR_SUB, 5)), Some(CUR_SUB), "deltas", CUR_SUB, 9, 4),
        Resync
    );
    // Tolerated-but-unapplied payload kind (PARITY.md §6 keeps unknown
    // enum values non-fatal).
    assert_eq!(
        classify_frame(
            Some((CUR_SUB, 5)),
            Some(CUR_SUB),
            "something-new",
            CUR_SUB,
            5,
            7
        ),
        Drop
    );
}
