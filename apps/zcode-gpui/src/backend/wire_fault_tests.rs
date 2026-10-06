//! Wire assembler fault-case tests (split from wire_tests.rs for the
//! 400-line cap): limits, conflicts, and strict field validation, mirroring
//! the canonical TypeScript assembler's required behaviors.

use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;

use super::helpers::*;

#[test]
fn rejects_fragment_count_and_size_limits() {
    let mut asm = FrameAssembler::new();
    let one = b"x".to_vec();
    let crc = crc32fast::hash(&one);
    let err = asm
        .ingest(&fragment_params("t", "s", 1, "f", 0, 1025, 1025, &one, crc))
        .unwrap_err();
    assert!(err.contains("FragmentCountExceeded"), "{err}");
    assert!(asm.assemblies.is_empty());
    // Fresh assembler: the count fault above settled the route tombstone.
    let mut asm = FrameAssembler::new();
    let err = asm
        .ingest(&fragment_params(
            "t",
            "s",
            1,
            "f",
            0,
            1,
            MAX_ASSEMBLY_BYTES + 1,
            &one,
            crc,
        ))
        .unwrap_err();
    assert!(err.contains("TooLarge"), "{err}");
    // No allocation happened for rejected groups: the map stays empty.
    assert!(asm.assemblies.is_empty());
}

#[test]
fn rejects_fragment_metadata_conflicts() {
    let mut asm = FrameAssembler::new();
    let a = b"aaaa".to_vec();
    let crc = crc32fast::hash(&a);
    assert!(
        asm.ingest(&fragment_params("t", "s", 1, "f", 0, 2, 8, &a, crc))
            .unwrap()
            .is_none()
    );
    // Same ordinal, different declared count → metadata conflict.
    let b = b"bbbb".to_vec();
    let err = asm
        .ingest(&fragment_params("t", "s", 1, "f", 1, 3, 12, &b, crc))
        .unwrap_err();
    assert!(err.contains("metadata conflict"), "{err}");
    // A conflicting id at the live ordinal is a conflict, not a replace.
    let err = asm
        .ingest(&fragment_params("t", "s", 1, "f-x", 1, 2, 8, &b, crc))
        .unwrap_err();
    assert!(err.contains("OrdinalConflict"), "{err}");
}

#[test]
fn duplicate_fragments() {
    let mut asm = FrameAssembler::new();
    let a = b"aaaa".to_vec();
    let b = b"bbbb".to_vec();
    let crc = crc32fast::hash(&[a.as_slice(), b.as_slice()].concat());
    let p0 = fragment_params("t", "s", 1, "f", 0, 2, 8, &a, crc);
    assert!(asm.ingest(&p0).unwrap().is_none());
    // Equal bytes: benign replay.
    assert!(asm.ingest(&p0).unwrap().is_none());
    // Different bytes at the same index: conflict.
    let err = asm
        .ingest(&fragment_params("t", "s", 1, "f", 0, 2, 8, &b, crc))
        .unwrap_err();
    assert!(err.contains("FragmentConflict"), "{err}");
}

#[test]
fn rejects_oversized_physical_envelope() {
    let mut asm = FrameAssembler::new();
    let big = vec![b'a'; MAX_FRAME_BYTES];
    let params = serde_json::json!({
        "wireVersion": 3, "kind": "fragment", "deliveryKind": "initial",
        "logicalFrameId": "f", "logicalFrameOrdinal": 1,
        "topic": "t", "subscriptionId": "s",
        "fragmentIndex": 0, "fragmentCount": 1, "logicalBytes": 1,
        "checksum": { "algorithm": "crc32", "value": "00000000" },
        "dataBase64": B64.encode(&big),
    });
    let err = asm.ingest(&params).unwrap_err();
    assert!(err.contains("EnvelopeTooLarge"), "{err}");
}

#[test]
fn rejects_strict_field_violations() {
    // A fresh assembler per case: every fault settles the route's tombstone,
    // and a settled ordinal would swallow the next same-ordinal case.
    let one = b"x".to_vec();
    let crc = crc32fast::hash(&one);
    // wireVersion must be exactly 3.
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc);
    p["wireVersion"] = serde_json::json!(2);
    assert!(asm.ingest(&p).unwrap_err().contains("wireVersion"));
    // deliveryKind is a closed enum.
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc);
    p["deliveryKind"] = serde_json::json!("sneaky");
    assert!(asm.ingest(&p).unwrap_err().contains("deliveryKind"));
    // Missing logicalBytes is a fault, not a zero default.
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc);
    p.as_object_mut().unwrap().remove("logicalBytes");
    assert!(asm.ingest(&p).unwrap_err().contains("logicalBytes"));
    // Index out of range.
    let mut asm = FrameAssembler::new();
    let err = asm
        .ingest(&fragment_params("t", "s", 1, "f", 1, 1, 1, &one, crc))
        .unwrap_err();
    assert!(err.contains("bad fragment fields"), "{err}");
    // Non-canonical base64 length (not a multiple of 4).
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc);
    p["dataBase64"] = serde_json::json!("AAA");
    assert!(asm.ingest(&p).unwrap_err().contains("InvalidBase64"));
    // Checksum must be crc32 + 8 hex chars.
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc);
    p["checksum"] = serde_json::json!({ "algorithm": "md5", "value": "zz" });
    assert!(asm.ingest(&p).unwrap_err().contains("bad fragment fields"));
    // Zero/negative ordinal is rejected (the canonical schema requires >= 1).
    let mut asm = FrameAssembler::new();
    let mut p = fragment_params("t", "s", 0, "f", 0, 1, 1, &one, crc);
    p["logicalFrameOrdinal"] = serde_json::json!(0);
    assert!(asm.ingest(&p).unwrap_err().contains("positive"));
}

#[test]
fn complete_frame_enforces_inner_envelope_and_seq_shape() {
    // Fresh assembler per case (faults tombstone the route ordinal).
    // Inner topic must match the envelope's topic.
    let mut asm = FrameAssembler::new();
    let err = asm
        .ingest(&complete_params(
            "t",
            "s",
            1,
            "f",
            logical_frame("other-topic", "s", 0, 1),
        ))
        .unwrap_err();
    assert!(err.contains("route mismatch"), "{err}");
    // Inner logical frame requires fromSeq/toSeq/payload with a kind.
    let mut asm = FrameAssembler::new();
    let err = asm
        .ingest(&complete_params(
            "t",
            "s",
            1,
            "f",
            serde_json::json!({ "topic": "t", "subscriptionId": "s" }),
        ))
        .unwrap_err();
    assert!(err.contains("bad logical frame"), "{err}");
    // toSeq must not go backwards.
    let mut asm = FrameAssembler::new();
    let err = asm
        .ingest(&complete_params(
            "t",
            "s",
            1,
            "f",
            logical_frame("t", "s", 5, 4),
        ))
        .unwrap_err();
    assert!(err.contains("toSeq < fromSeq"), "{err}");
}

#[test]
fn concurrent_assembly_limit() {
    let mut asm = FrameAssembler::new();
    let one = b"x".to_vec();
    let crc = crc32fast::hash(&one);
    for i in 0..MAX_CONCURRENT_ASSEMBLIES {
        let topic = format!("t{i}");
        assert!(
            asm.ingest(&fragment_params(&topic, "s", 1, "f", 0, 2, 2, &one, crc))
                .unwrap()
                .is_none()
        );
    }
    let err = asm
        .ingest(&fragment_params(
            "overflow-topic",
            "s",
            1,
            "f",
            0,
            2,
            2,
            &one,
            crc,
        ))
        .unwrap_err();
    assert!(err.contains("ConcurrentLimit"), "{err}");
}

// --- Review finding 4/5 regression tests (2026-10-05) -----------------------

use std::time::Duration;

#[test]
fn global_staging_budget_covers_later_fragments() {
    // Finding 4: the budget must hold for later fragments, not only for
    // assembly creation. A 16-byte budget: assembly A stages 10; B's first
    // fragment of 7 must fault instead of staging 17 total.
    let mut asm = FrameAssembler::with_limits(16, ASSEMBLY_TIMEOUT);
    let ten = [b'a'; 10];
    let seven = [b'b'; 7];
    let crc_a = crc32fast::hash(&ten);
    let crc_b = crc32fast::hash(&seven);
    // Assembly A: fragment 0 of 2 stages 10 bytes and stays incomplete.
    assert!(
        asm.ingest(&fragment_params("tA", "s", 1, "f", 0, 2, 10, &ten, crc_a))
            .unwrap()
            .is_none()
    );
    // Assembly B's first fragment (7 bytes) crosses the 16-byte global
    // budget: fault instead of staging 17 total.
    let err = asm
        .ingest(&fragment_params("tB", "s", 1, "f", 0, 2, 14, &seven, crc_b))
        .unwrap_err();
    assert!(err.contains("BudgetExceeded"), "{err}");
    // The faulted assembly is released: bytes return to the budget and only
    // A's assembly remains.
    assert_eq!(asm.staged_decoded_bytes, 10);
    assert_eq!(asm.assemblies.len(), 1);
}

#[test]
fn checksum_requires_lowercase_hex() {
    let mut asm = FrameAssembler::new();
    let one = b"x".to_vec();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, 0xdeadbeef);
    // Canonical schema demands ^[0-9a-f]{8}$: uppercase is a fault, not a
    // normalizable variant (review finding 5).
    p["checksum"]["value"] = serde_json::json!("DEADBEEF");
    assert!(asm.ingest(&p).unwrap_err().contains("bad fragment fields"));
}

#[test]
fn extra_physical_keys_are_rejected() {
    let mut asm = FrameAssembler::new();
    let one = b"x".to_vec();
    let mut p = fragment_params("t", "s", 1, "f", 0, 1, 1, &one, crc32fast::hash(&one));
    p["sneakyExtra"] = serde_json::json!(1);
    let err = asm.ingest(&p).unwrap_err();
    assert!(err.contains("unexpected envelope key"), "{err}");
    // Complete frames enforce the same strictness.
    let mut c = complete_params("t", "s", 1, "f", logical_frame("t", "s", 0, 1));
    c["sneakyExtra"] = serde_json::json!(1);
    assert!(
        asm.ingest(&c)
            .unwrap_err()
            .contains("unexpected envelope key")
    );
}

#[test]
fn tombstones_never_move_backward() {
    let mut asm = FrameAssembler::new();
    // Settle ordinal 5.
    assert!(
        asm.ingest(&complete_params(
            "t",
            "s",
            5,
            "f-5",
            logical_frame("t", "s", 0, 1)
        ))
        .unwrap()
        .is_some()
    );
    // A malformed stale frame (ordinal 3, invalid deliveryKind) is dropped
    // as STALE before field validation — the tombstone, not the first
    // malformed field, decides staleness (review finding 5).
    let mut stale = complete_params("t", "s", 3, "f-3", logical_frame("t", "s", 0, 1));
    stale["deliveryKind"] = serde_json::json!("sneaky");
    assert!(asm.ingest(&stale).unwrap().is_none());
    // Nothing below the tombstone can settle at all (staleness is checked
    // before any fault path), so a lower ordinal can never drag it
    // backward: ordinal 4 stays silently stale.
    let four = complete_params("t", "s", 4, "f-4", logical_frame("t", "s", 0, 1));
    assert!(asm.ingest(&four).unwrap().is_none());
    // Faults above the tombstone still settle forward: a route-mismatch
    // fault at ordinal 6 tombstones (6, f-6), so an exact replay of 6 is
    // dropped instead of being applied twice.
    let bad6 = complete_params("t", "s", 6, "f-6", logical_frame("other", "s", 0, 1));
    assert!(asm.ingest(&bad6).unwrap_err().contains("route mismatch"));
    let six = complete_params("t", "s", 6, "f-6", logical_frame("t", "s", 0, 1));
    assert!(asm.ingest(&six).unwrap().is_none());
}

#[test]
fn timed_out_assemblies_surface_for_resync() {
    // Finding 5: the sweep must return the expired topics (events.rs resyncs
    // them); the caller-side sweep replaced the swallowed in-ingest sweep.
    let mut asm = FrameAssembler::with_limits(MAX_STAGED_DECODED_BYTES, Duration::from_millis(5));
    let chunk = b"aaaa".to_vec();
    let crc = crc32fast::hash(&[chunk.as_slice(), chunk.as_slice()].concat());
    assert!(
        asm.ingest(&fragment_params("t", "s", 1, "f", 0, 2, 8, &chunk, crc))
            .unwrap()
            .is_none()
    );
    std::thread::sleep(Duration::from_millis(15));
    assert_eq!(asm.sweep_timeouts(), vec!["t".to_string()]);
    assert!(asm.assemblies.is_empty());
    assert_eq!(asm.staged_decoded_bytes, 0);
}

#[test]
fn fragment_frames_reject_backwards_seq_ranges() {
    // Finding 5: assembled fragments enforce toSeq >= fromSeq just like
    // complete frames.
    let mut asm = FrameAssembler::new();
    let frame = logical_frame("t", "s", 9, 4);
    let bytes = frame.to_string().into_bytes();
    let crc = crc32fast::hash(&bytes);
    let err = send_fragments(&mut asm, "t", "s", 1, 2, &bytes, crc).unwrap_err();
    assert!(err.contains("toSeq < fromSeq"), "{err}");
}
