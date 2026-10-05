//! Golden tests for `backend/wire.rs` happy paths and tombstone behavior
//! (kept out-of-line for the 400-line cap; fault cases live in
//! wire_fault_tests.rs).

use super::*;

use super::helpers::*;

#[test]
fn parses_all_envelope_shapes() {
    match parse_line(r#"{"id":1,"method":"m","params":{}}"#).unwrap() {
        Incoming::AgentRequest { id, method, .. } => {
            assert_eq!(id, serde_json::json!(1));
            assert_eq!(method, "m");
        }
        _ => panic!("request misparsed"),
    }
    // Non-integer agent ids must still be recognized as requests (the
    // agent uses its own id space) — regression for the -32601 timeout.
    match parse_line(r#"{"id":"srv-7","method":"m"}"#).unwrap() {
        Incoming::AgentRequest { id, .. } => assert_eq!(id, serde_json::json!("srv-7")),
        _ => panic!("string-id request misparsed"),
    }
    match parse_line(r#"{"id":2,"result":{"ok":true}}"#).unwrap() {
        Incoming::Response { id, result, error } => {
            assert_eq!(id, 2);
            assert!(result.unwrap()["ok"] == serde_json::json!(true));
            assert!(error.is_none());
        }
        _ => panic!("response misparsed"),
    }
    match parse_line(r#"{"id":3,"error":{"code":-1,"message":"x"}}"#).unwrap() {
        Incoming::Response { error, .. } => assert!(error.is_some()),
        _ => panic!("error response misparsed"),
    }
    match parse_line(r#"{"method":"startup/storageState","params":{"phase":"ready"}}"#).unwrap() {
        Incoming::Notification { method, params } => {
            assert_eq!(method, "startup/storageState");
            assert!(params["phase"] == serde_json::json!("ready"));
        }
        _ => panic!("notification misparsed"),
    }
    assert!(parse_line("not json").is_none());
}

#[test]
fn reassembles_fragments_and_verifies_crc() {
    let frame = serde_json::json!({
        "topic": "conversation/s1", "subscriptionId": "sub-1",
        "fromSeq": 0, "toSeq": 3,
        "payload": { "kind": "snapshot", "snapshot": { "revision": 7 } }
    });
    let bytes = frame.to_string().into_bytes();
    let crc = crc32fast::hash(&bytes);
    let mut asm = FrameAssembler::new();
    let done = send_fragments(&mut asm, "conversation/s1", "sub-1", 1, 3, &bytes, crc)
        .unwrap()
        .expect("frame should complete");
    assert_eq!(done.topic, "conversation/s1");
    assert!(done.payload["snapshot"]["revision"] == serde_json::json!(7));
}

#[test]
fn rejects_fragment_with_bad_crc() {
    let bytes = b"hello world".to_vec();
    let mut asm = FrameAssembler::new();
    let err = send_fragments(&mut asm, "t", "s", 1, 2, &bytes, 0xdeadbeef).unwrap_err();
    assert!(err.contains("ChecksumMismatch"), "{err}");
}

#[test]
fn crc_failure_settles_the_route_against_replay() {
    let bytes = b"hello world".to_vec();
    let mut asm = FrameAssembler::new();
    let _ = send_fragments(&mut asm, "t", "s", 1, 2, &bytes, 0xdeadbeef);
    // A replay of the same (ordinal, id) after the fault is tombstoned away.
    let replay = asm.ingest(&fragment_params(
        "t",
        "s",
        1,
        "f-1",
        0,
        2,
        bytes.len(),
        &bytes[..6],
        0xdeadbeef,
    ));
    assert!(replay.unwrap().is_none());
}

#[test]
fn ordinals_act_as_tombstones() {
    let mut asm = FrameAssembler::new();
    assert!(
        asm.ingest(&complete_params(
            "t",
            "s",
            2,
            "f-2",
            logical_frame("t", "s", 5, 6)
        ))
        .unwrap()
        .is_some()
    );
    // A late fragment group with a lower ordinal is dropped silently.
    let bytes = b"{}".to_vec();
    let crc = crc32fast::hash(&bytes);
    assert!(
        asm.ingest(&fragment_params(
            "t",
            "s",
            1,
            "f-1",
            0,
            1,
            bytes.len(),
            &bytes,
            crc
        ))
        .unwrap()
        .is_none()
    );
    // Same ordinal, different id → conflict, never a resurrection.
    let err = asm
        .ingest(&complete_params(
            "t",
            "s",
            2,
            "f-2b",
            logical_frame("t", "s", 5, 6),
        ))
        .unwrap_err();
    assert!(err.contains("OrdinalConflict"), "{err}");
    // Same ordinal, same id → exact replay, dropped.
    assert!(
        asm.ingest(&complete_params(
            "t",
            "s",
            2,
            "f-2",
            logical_frame("t", "s", 5, 6)
        ))
        .unwrap()
        .is_none()
    );
}

#[test]
fn complete_frames_pass_through() {
    let mut asm = FrameAssembler::new();
    let frame = asm
        .ingest(&complete_params(
            "sessions-index/W",
            "s",
            1,
            "f-1",
            logical_frame("sessions-index/W", "s", 0, 0),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(frame.topic, "sessions-index/W");
}

#[test]
fn forget_topics_drops_partial_groups() {
    let mut asm = FrameAssembler::new();
    let a = b"aaaa".to_vec();
    let crc = crc32fast::hash(&[a.as_slice(), a.as_slice()].concat());
    assert!(
        asm.ingest(&fragment_params("t", "s", 1, "f", 0, 2, 8, &a, crc))
            .unwrap()
            .is_none()
    );
    asm.forget_topics(&["t".to_string()]);
    // After the forget, the same fragment starts a fresh group (no conflict).
    assert!(
        asm.ingest(&fragment_params("t", "s", 1, "f", 0, 2, 8, &a, crc))
            .unwrap()
            .is_none()
    );
    assert!(asm.settled.is_empty());
}
