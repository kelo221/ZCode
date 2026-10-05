//! Shared fixture builders for the wire assembler test modules
//! (wire_tests.rs / wire_fault_tests.rs).

#![allow(dead_code)]

use crate::backend::wire::{FrameAssembler, LogicalFrame};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::Value;

#[allow(clippy::too_many_arguments)]
pub fn fragment_params(
    topic: &str,
    sub: &str,
    ordinal: u64,
    logical_id: &str,
    index: u32,
    count: u32,
    logical_bytes: usize,
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
        "logicalBytes": logical_bytes,
        "checksum": { "algorithm": "crc32", "value": format!("{crc:08x}") },
        "dataBase64": B64.encode(chunk),
    })
}

pub fn complete_params(
    topic: &str,
    sub: &str,
    ordinal: u64,
    logical_id: &str,
    frame: Value,
) -> Value {
    serde_json::json!({
        "wireVersion": 3, "kind": "complete", "deliveryKind": "online",
        "logicalFrameId": logical_id, "logicalFrameOrdinal": ordinal,
        "topic": topic, "subscriptionId": sub,
        "frame": frame
    })
}

pub fn logical_frame(topic: &str, sub: &str, from: u64, to: u64) -> Value {
    serde_json::json!({
        "topic": topic, "subscriptionId": sub,
        "fromSeq": from, "toSeq": to,
        "payload": { "kind": "deltas", "deltas": [] }
    })
}

/// Send `bytes` as `parts` fragments; returns the last ingest result.
pub fn send_fragments(
    asm: &mut FrameAssembler,
    topic: &str,
    sub: &str,
    ordinal: u64,
    parts: usize,
    bytes: &[u8],
    crc: u32,
) -> Result<Option<LogicalFrame>, String> {
    let n = bytes.len() / parts;
    let mut last = Ok(None);
    for i in 0..parts {
        let start = i * n;
        let end = if i == parts - 1 {
            bytes.len()
        } else {
            start + n
        };
        last = asm.ingest(&fragment_params(
            topic,
            sub,
            ordinal,
            "f-1",
            i as u32,
            parts as u32,
            bytes.len(),
            &bytes[start..end],
            crc,
        ));
    }
    last
}
