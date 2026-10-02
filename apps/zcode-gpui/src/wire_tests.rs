//! Golden tests for `wire.rs` (kept out-of-line for the 400-line cap).

use super::*;
use base64::engine::general_purpose::STANDARD as B64;

#[allow(clippy::too_many_arguments)]
fn fragment_params(
    topic: &str,
    sub: &str,
    ordinal: u64,
    logical_id: &str,
    index: u32,
    count: u32,
    chunk: &[u8],
    crc: u32,
) -> Value {
    serde_json::json!({
        "wireVersion": 3,
        "kind": "fragment",
        "deliveryKind": "initial",
        "logicalFrameId": logical_id,
        "logicalFrameOrdinal": ordinal,
        "topic": topic,
        "subscriptionId": sub,
        "fragmentIndex": index,
        "fragmentCount": count,
        "checksum": { "algorithm": "crc32", "value": format!("{crc:08x}") },
        "dataBase64": B64.encode(chunk),
    })
}

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
            assert_eq!(result.unwrap()["ok"], serde_json::json!(true));
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
            assert_eq!(params["phase"], serde_json::json!("ready"));
        }
        _ => panic!("notification misparsed"),
    }
    assert!(parse_line("not json").is_none());
}

#[test]
fn reassembles_fragments_and_verifies_crc() {
    let frame = serde_json::json!({
        "topic": "conversation/s1",
        "subscriptionId": "sub-1",
        "fromSeq": 0,
        "toSeq": 3,
        "payload": { "kind": "snapshot", "snapshot": { "revision": 7 } }
    });
    let bytes = frame.to_string().into_bytes();
    let crc = crc32fast::hash(&bytes);
    let (a, b, c) = (&bytes[..5], &bytes[5..20], &bytes[20..]);
    let mut asm = FrameAssembler::new();
    assert!(
        asm.ingest(&fragment_params(
            "conversation/s1",
            "sub-1",
            1,
            "f-1",
            0,
            3,
            a,
            crc
        ))
        .unwrap()
        .is_none()
    );
    assert!(
        asm.ingest(&fragment_params(
            "conversation/s1",
            "sub-1",
            1,
            "f-1",
            1,
            3,
            b,
            crc
        ))
        .unwrap()
        .is_none()
    );
    let done = asm
        .ingest(&fragment_params(
            "conversation/s1",
            "sub-1",
            1,
            "f-1",
            2,
            3,
            c,
            crc,
        ))
        .unwrap()
        .expect("frame should complete");
    assert_eq!(done.topic, "conversation/s1");
    assert_eq!(done.payload["snapshot"]["revision"], serde_json::json!(7));
}

#[test]
fn rejects_fragment_with_bad_crc() {
    let bytes = b"hello world".to_vec();
    let mut asm = FrameAssembler::new();
    let _ = asm.ingest(&fragment_params(
        "t",
        "s",
        1,
        "f",
        0,
        2,
        &bytes[..6],
        0xdeadbeef,
    ));
    let err = asm
        .ingest(&fragment_params(
            "t",
            "s",
            1,
            "f",
            1,
            2,
            &bytes[6..],
            0xdeadbeef,
        ))
        .unwrap_err();
    assert!(err.contains("CRC mismatch"));
}

#[test]
fn ordinals_act_as_tombstones() {
    let complete = serde_json::json!({
        "wireVersion": 3, "kind": "complete", "deliveryKind": "online",
        "logicalFrameId": "f-2", "logicalFrameOrdinal": 2,
        "topic": "t", "subscriptionId": "s",
        "frame": { "topic": "t", "subscriptionId": "s", "fromSeq": 5, "toSeq": 6,
                   "payload": { "kind": "deltas", "deltas": [] } }
    });
    let mut asm = FrameAssembler::new();
    assert!(asm.ingest(&complete).unwrap().is_some());
    // A late fragment group with a lower ordinal is dropped silently.
    let bytes = b"{}".to_vec();
    let crc = crc32fast::hash(&bytes);
    assert!(
        asm.ingest(&fragment_params("t", "s", 1, "f-1", 0, 1, &bytes, crc))
            .unwrap()
            .is_none()
    );
}

#[test]
fn complete_frames_pass_through() {
    let complete = serde_json::json!({
        "wireVersion": 3, "kind": "complete", "deliveryKind": "initial",
        "logicalFrameId": "f-1", "logicalFrameOrdinal": 1,
        "topic": "sessions-index/W", "subscriptionId": "s",
        "frame": { "topic": "sessions-index/W", "subscriptionId": "s",
                   "fromSeq": 0, "toSeq": 0, "payload": { "kind": "snapshot", "snapshot": {} } }
    });
    let mut asm = FrameAssembler::new();
    let frame = asm.ingest(&complete).unwrap().unwrap();
    assert_eq!(frame.topic, "sessions-index/W");
}
