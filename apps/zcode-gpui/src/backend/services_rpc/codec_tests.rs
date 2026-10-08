use super::{codec::*, frame::*, message::*};
use serde_json::json;
use std::io::{Cursor, Read};

#[test]
fn authoritative_initialize_and_proxy_request_bytes() {
    let initialize = vec![4, 1, 6, 200, 1, 0];
    assert_eq!(decode_message(&initialize).unwrap(), Message::Initialize);
    assert_eq!(
        encode_message(&[200], WireValue::Undefined).unwrap(),
        initialize
    );
    let request = request_frame(0, "s", "m", vec![json!(null)]).unwrap();
    assert_eq!(
        request,
        vec![
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 20, 4, 4, 6, 100, 6, 0, 1, 1, b's', 1, 1, b'm', 4,
            1, 5, 4, b'n', b'u', b'l', b'l',
        ]
    );
    let cancel = cancel_frame(128).unwrap();
    assert_eq!(&cancel[13..], &[4, 2, 6, 101, 6, 128, 1, 0]);
}

#[test]
fn vql_signed_ints_and_all_tags_follow_ts() {
    for (value, bytes) in [
        (0, vec![6, 0]),
        (127, vec![6, 127]),
        (128, vec![6, 128, 1]),
        (-1, vec![6, 255, 255, 255, 255, 15]),
        (i32::MIN, vec![6, 128, 128, 128, 128, 8]),
    ] {
        let wire = WireValue::Json(json!(value));
        assert_eq!(encode_value(&wire).unwrap(), bytes);
        assert_eq!(decode_value(&bytes).unwrap(), wire);
    }
    for bytes in [vec![2, 2, 0, 255], vec![3, 2, 0, 255]] {
        assert_eq!(encode_value(&decode_value(&bytes).unwrap()).unwrap(), bytes);
    }
    for value in [
        json!(true),
        json!(false),
        json!(null),
        json!(1.25),
        json!(2147483648_u64),
        json!({"nested": ["中文", null, 1.2]}),
    ] {
        let wire = WireValue::Json(value);
        assert_eq!(decode_value(&encode_value(&wire).unwrap()).unwrap(), wire);
    }
    assert_eq!(
        encode_value(&WireValue::Json(json!(1.0))).unwrap(),
        vec![6, 1]
    );
}

#[test]
fn undefined_is_not_json_null_and_void_is_success() {
    assert_eq!(decode_value(&[0]).unwrap(), WireValue::Undefined);
    assert_eq!(
        encode_value(&WireValue::Json(json!(null))).unwrap(),
        b"\x05\x04null"
    );
    assert_eq!(
        decode_message(&encode_message(&[201, 7], WireValue::Undefined).unwrap()).unwrap(),
        Message::Success(7, ServiceValue::Undefined)
    );
    assert_eq!(
        decode_message(&encode_message(&[201, 7], WireValue::Json(json!(null))).unwrap()).unwrap(),
        Message::Success(7, ServiceValue::Json(json!(null)))
    );
    assert!(ServiceValue::Undefined.into_json().is_err());
    assert_eq!(
        ServiceValue::Json(json!(null)).into_json().unwrap(),
        json!(null)
    );
}

struct Fragmented(Cursor<Vec<u8>>);
impl Read for Fragmented {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = buf.len().min(1);
        self.0.read(&mut buf[..len])
    }
}

#[test]
fn fragmented_and_coalesced_frames_preserve_headers() {
    let payload = encode_message(&[200], WireValue::Undefined).unwrap();
    let frame = wrap_frame(&payload).unwrap();
    let mut bytes = frame.clone();
    bytes.extend(&frame);
    let mut reader = Fragmented(Cursor::new(bytes));
    for _ in 0..2 {
        let got = read_frame(&mut reader).unwrap().unwrap();
        assert_eq!(got.kind, 1);
        assert_eq!(got.payload, payload);
    }
    assert!(read_frame(&mut reader).unwrap().is_none());
    for cut in 1..frame.len() {
        assert!(read_frame(&mut Cursor::new(&frame[..cut])).is_err());
    }
}

#[test]
fn malformed_and_oversized_wire_fails_before_allocation() {
    for bytes in [
        vec![],
        vec![7],
        vec![1],
        vec![1, 2, b'a'],
        vec![4, 1],
        vec![6, 128],
        vec![6, 255, 255, 255, 255, 31],
        vec![6, 128, 128, 128, 128, 128, 0],
        vec![5, 1, b'{'],
        vec![0, 0],
        vec![4, 255, 255, 255, 255, 15],
        vec![1, 255, 255, 255, 255, 15],
    ] {
        assert!(decode_value(&bytes).is_err(), "accepted {bytes:?}");
    }
    let mut frame = vec![0; 13];
    frame[0] = 1;
    frame[9..].copy_from_slice(&((MAX_FRAME_BYTES + 1) as u32).to_be_bytes());
    assert!(read_frame(&mut Cursor::new(frame)).is_err());
    let text = WireValue::Json(json!("x".repeat(MAX_TEXT_BYTES + 1)));
    assert!(encode_value(&text).is_err());
    let huge_array = WireValue::Array(vec![WireValue::Undefined; MAX_COLLECTION_ITEMS + 1]);
    assert!(encode_value(&huge_array).is_err());
    let mut nested = WireValue::Undefined;
    for _ in 0..MAX_DEPTH + 2 {
        nested = WireValue::Array(vec![nested]);
    }
    assert!(encode_value(&nested).is_err());
    let mut nested_bytes = vec![];
    for _ in 0..MAX_DEPTH + 2 {
        nested_bytes.extend([4, 1]);
    }
    nested_bytes.push(0);
    assert!(decode_value(&nested_bytes).is_err());
}

#[test]
fn json_fallback_is_depth_bounded_and_string_brackets_are_ignored() {
    let mut nested = json!(null);
    for _ in 0..MAX_DEPTH + 2 {
        nested = json!({"x": nested});
    }
    assert!(encode_value(&WireValue::Json(nested)).is_err());
    let mut text = String::new();
    for _ in 0..MAX_DEPTH + 2 {
        text.push('[');
    }
    text.push('0');
    for _ in 0..MAX_DEPTH + 2 {
        text.push(']');
    }
    let mut bytes = vec![5, text.len() as u8];
    bytes.extend(text.as_bytes());
    assert!(decode_value(&bytes).is_err());
    let brackets = WireValue::Json(json!({"text": "[\\\"{}]"}));
    assert_eq!(
        decode_value(&encode_value(&brackets).unwrap()).unwrap(),
        brackets
    );
}

#[test]
fn json_surface_rejects_reserved_nested_binary_markers() {
    let marker = json!({"__zcode_rpc_nested_uint8array_v1": true, "base64": "AA=="});
    assert!(encode_value(&WireValue::Json(json!({"nested":marker}))).is_err());
    let text = serde_json::to_vec(&marker).unwrap();
    assert!(text.len() < 128);
    let mut bytes = vec![5, text.len() as u8];
    bytes.extend(text);
    assert!(decode_value(&bytes).is_err());
}

#[test]
fn response_validation_and_error_redaction() {
    let secret = "synthetic-secret@example.invalid";
    for kind in [202, 203] {
        let response = encode_message(
            &[kind, 9],
            WireValue::Json(json!({
                "message":secret, "name":"Error", "stack":[secret], "data":secret
            })),
        )
        .unwrap();
        let Message::Failure(9, error) = decode_message(&response).unwrap() else {
            panic!()
        };
        assert!(!error.contains(secret));
        assert!(error.len() < 256);
    }
    for bytes in [
        encode_message(&[200], WireValue::Json(json!(null))).unwrap(),
        encode_message(&[201], WireValue::Undefined).unwrap(),
        encode_message(&[999, 0], WireValue::Undefined).unwrap(),
        encode_message(&[201, u32::MAX], WireValue::Undefined).unwrap(),
    ] {
        assert!(decode_message(&bytes).is_err());
    }
}
