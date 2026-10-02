//! Wire layer: NDJSON RPC envelope parsing and V4 topic-frame assembly.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (wire.ts, wire-codec.ts,
//! wire-binary.ts) and packages/services/src/zcode-agent/zcodeStdioTransport.ts
//! (LF-delimited JSON lines, one object per line, UTF-8).

use base64::Engine;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// One parsed inbound NDJSON message.
pub enum Incoming {
    Response {
        id: u64,
        result: Option<Value>,
        error: Option<Value>,
    },
    /// Server→client request (has both id and method); the client must reply.
    /// The id is echoed back verbatim — the agent uses its own id space, which
    /// is not guaranteed to be integers. Params are not interpreted by the
    /// minimal client: it answers -32601.
    AgentRequest {
        id: Value,
        method: String,
        #[allow(dead_code)]
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

pub fn parse_line(line: &str) -> Option<Incoming> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let method = v.get("method").and_then(Value::as_str);
    if let Some(method) = method {
        return Some(match v.get("id") {
            Some(id) if !id.is_null() => Incoming::AgentRequest {
                id: id.clone(),
                method: method.to_string(),
                params: v.get("params").cloned().unwrap_or(Value::Null),
            },
            _ => Incoming::Notification {
                method: method.to_string(),
                params: v.get("params").cloned().unwrap_or(Value::Null),
            },
        });
    }
    let id = v.get("id").and_then(Value::as_u64)?;
    Some(Incoming::Response {
        id,
        result: v.get("result").cloned(),
        error: v.get("error").cloned(),
    })
}

/// A reassembled logical topic frame: (fromSeq, toSeq] range + payload.
#[derive(Debug)]
pub struct LogicalFrame {
    pub topic: String,
    /// Routing is by topic prefix today; the id matters for multi-subscription
    /// clients and frame buffering until the subscribe ack.
    #[allow(dead_code)]
    pub subscription_id: String,
    pub from_seq: u64,
    pub to_seq: u64,
    pub payload: Value,
}

fn logical_from_value(frame: &Value) -> Option<LogicalFrame> {
    Some(LogicalFrame {
        topic: frame.get("topic")?.as_str()?.to_string(),
        subscription_id: frame
            .get("subscriptionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        from_seq: frame.get("fromSeq").and_then(Value::as_u64).unwrap_or(0),
        to_seq: frame.get("toSeq").and_then(Value::as_u64).unwrap_or(0),
        payload: frame.get("payload").cloned().unwrap_or(Value::Null),
    })
}

struct Assembly {
    ordinal: u64,
    count: u32,
    parts: Vec<Option<Vec<u8>>>,
    crc: Option<u32>,
    started: Instant,
}

/// Reassembles physical wire frames (complete | fragment) into logical frames.
///
/// Fragmentation is real over stdio (the encoder budget is dominated by the
/// mobile-relay transport), so this is not optional. CRC-32 (IEEE, reflected)
/// covers the whole logical JSON; fragments carry base64 slices.
pub struct FrameAssembler {
    assemblies: HashMap<(String, String), Assembly>,
    last_ordinal: HashMap<(String, String), u64>,
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self {
            assemblies: HashMap::new(),
            last_ordinal: HashMap::new(),
        }
    }

    /// Ingest the params of a `v4/conversation/frame` notification.
    /// Returns a logical frame when one fully arrived; Ok(None) for tombstoned
    /// ordinals or incomplete fragment groups.
    pub fn ingest(&mut self, params: &Value) -> Result<Option<LogicalFrame>, String> {
        self.sweep_timeouts();
        let kind = params
            .get("kind")
            .and_then(Value::as_str)
            .ok_or("frame missing kind")?;
        let topic = params
            .get("topic")
            .and_then(Value::as_str)
            .ok_or("frame missing topic")?
            .to_string();
        let sub = params
            .get("subscriptionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ordinal = params
            .get("logicalFrameOrdinal")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let key = (topic.clone(), sub.clone());

        // Ordinal is a tombstone: anything below the last settled ordinal is dropped.
        if let Some(last) = self.last_ordinal.get(&key)
            && ordinal < *last
        {
            return Ok(None);
        }

        match kind {
            "complete" => {
                let frame = params.get("frame").ok_or("complete frame missing frame")?;
                let logical = logical_from_value(frame).ok_or("bad logical frame")?;
                self.last_ordinal.insert(key, ordinal);
                Ok(Some(logical))
            }
            "fragment" => {
                let logical_id = params
                    .get("logicalFrameId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let index = params
                    .get("fragmentIndex")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                let count = params
                    .get("fragmentCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(1) as u32;
                let data_b64 = params
                    .get("dataBase64")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let crc = params
                    .get("checksum")
                    .and_then(|c| c.get("value"))
                    .and_then(Value::as_str)
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok());

                let chunk = base64::engine::general_purpose::STANDARD
                    .decode(data_b64)
                    .map_err(|e| format!("fragment base64 decode failed: {e}"))?;

                let stale = match self.assemblies.get(&key) {
                    Some(a) => a.ordinal != ordinal || a.count != count,
                    None => true,
                };
                if stale {
                    self.assemblies.insert(
                        key.clone(),
                        Assembly {
                            ordinal,
                            count,
                            parts: vec![None; count as usize],
                            crc,
                            started: Instant::now(),
                        },
                    );
                }
                let assembly = self.assemblies.get_mut(&key).expect("just inserted");
                if (index as usize) < assembly.parts.len() {
                    assembly.parts[index as usize] = Some(chunk);
                }
                let all_present = assembly.parts.iter().all(|p| p.is_some());
                if !all_present {
                    return Ok(None);
                }
                let bytes: Vec<u8> = assembly
                    .parts
                    .iter()
                    .filter_map(|p| p.as_ref())
                    .flat_map(|c| c.iter().copied())
                    .collect();
                let crc = assembly.crc;

                if let Some(expected) = crc {
                    let actual = crc32fast::hash(&bytes);
                    if actual != expected {
                        self.assemblies.remove(&key);
                        return Err(format!(
                            "CRC mismatch on frame {logical_id} ({actual:08x} != {expected:08x})"
                        ));
                    }
                }
                self.assemblies.remove(&key);
                self.last_ordinal.insert(key, ordinal);

                let text = String::from_utf8(bytes).map_err(|e| format!("frame not utf-8: {e}"))?;
                let value: Value =
                    serde_json::from_str(&text).map_err(|e| format!("frame json invalid: {e}"))?;
                logical_from_value(&value)
                    .map(Some)
                    .ok_or_else(|| "bad logical frame".to_string())
            }
            other => Err(format!("unknown frame kind {other}")),
        }
    }

    /// Drop fragment groups stuck for >30s (the protocol's assembly cap) and
    /// return their topics so the caller can resync those routes.
    pub fn sweep_timeouts(&mut self) -> Vec<String> {
        let stale: Vec<(String, String)> = self
            .assemblies
            .iter()
            .filter(|(_, a)| a.started.elapsed() > Duration::from_secs(30))
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale {
            self.assemblies.remove(key);
        }
        stale.into_iter().map(|(topic, _)| topic).collect()
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
