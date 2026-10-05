//! Wire layer: NDJSON RPC envelope parsing and V4 topic-frame assembly.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (wire.ts, wire-codec.ts,
//! wire-binary.ts) and packages/services/src/zcode-agent/zcodeStdioTransport.ts
//! (LF-delimited JSON lines, one object per line, UTF-8).
//!
//! The assembler mirrors the canonical TypeScript `TopicWireFrameAssembler`
//! (packages/shared/src/zcode-protocol-v4/wire-assembler.ts): ordinal
//! tombstones, per-route metadata equality, and the `PROTOCOL_V4_LIMITS`
//! budgets are enforced before any logical frame is produced. Divergences are
//! limited to what the route layer already tolerates (unknown payload kinds
//! pass through; PARITY.md §6 keeps unknown enum values non-fatal).

use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// `PROTOCOL_V4_LIMITS.maxFrameBytes`: one NDJSON envelope (complete or
/// fragment) must not exceed 1 MiB. Enforced on the re-serialized params in
/// `ingest`, plus a cheaper raw-line guard in the event pump.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ASSEMBLY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_FRAGMENTS: usize = 1024;
pub(crate) const MAX_CONCURRENT_ASSEMBLIES: usize = 32;
pub(crate) const MAX_STAGED_DECODED_BYTES: usize = 32 * 1024 * 1024;
const ASSEMBLY_TIMEOUT: Duration = Duration::from_secs(30);
const WIRE_VERSION: u64 = 3;

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
    /// Generation identity of the subscription that delivered this frame; the
    /// route layer refuses frames whose generation differs from the applied
    /// cursor's (transport.ts: “代际标识，防旧流交错”).
    pub subscription_id: String,
    pub from_seq: u64,
    pub to_seq: u64,
    pub payload: Value,
}

/// Strict inner logical-frame parse: every field the route layer needs must be
/// present with the right type — missing fields are faults, not defaults.
pub(crate) fn logical_from_value(frame: &Value) -> Option<LogicalFrame> {
    let topic = frame.get("topic")?.as_str()?.to_string();
    let subscription_id = frame.get("subscriptionId")?.as_str()?.to_string();
    let from_seq = frame.get("fromSeq")?.as_u64()?;
    let to_seq = frame.get("toSeq")?.as_u64()?;
    let payload = frame.get("payload")?;
    payload.get("kind")?.as_str()?;
    Some(LogicalFrame {
        topic,
        subscription_id,
        from_seq,
        to_seq,
        payload: payload.clone(),
    })
}

pub(crate) struct Assembly {
    ordinal: u64,
    logical_frame_id: String,
    delivery_kind: &'static str,
    fragment_count: usize,
    logical_bytes: usize,
    /// 8 lowercase hex chars (crc32 of the whole logical JSON).
    checksum: String,
    fragments: Vec<Option<Vec<u8>>>,
    received: usize,
    decoded_bytes: usize,
    started: Instant,
}

/// Reassembles physical wire frames (complete | fragment) into logical frames.
///
/// Fragmentation is real over stdio (the encoder budget is dominated by the
/// mobile-relay transport), so this is not optional. CRC-32 (IEEE, reflected)
/// covers the whole logical JSON; fragments carry padded standard base64.
pub struct FrameAssembler {
    pub(super) assemblies: HashMap<(String, String), Assembly>,
    /// Per-route tombstone of the last settled (ordinal, logicalFrameId).
    /// Late arrivals below it are dropped; a conflicting id at the same
    /// ordinal is a fault — a settled frame can never be resurrected.
    settled: HashMap<(String, String), (u64, String)>,
    pub(super) staged_decoded_bytes: usize,
}

fn req_str<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Result<&'a str, String> {
    obj.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("proto.frameAssemblyMetadataMismatch: missing string {key}"))
}

fn req_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u64, String> {
    obj.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("proto.frameAssemblyMetadataMismatch: missing integer {key}"))
}

fn req_nonempty(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    let s = req_str(obj, key)?;
    if s.is_empty() {
        return Err(format!("proto.frameAssemblyMetadataMismatch: empty {key}"));
    }
    Ok(s.to_string())
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self {
            assemblies: HashMap::new(),
            settled: HashMap::new(),
            staged_decoded_bytes: 0,
        }
    }

    pub(super) fn settle(&mut self, key: &(String, String), ordinal: u64, id: &str) {
        self.settled.insert(key.clone(), (ordinal, id.to_string()));
    }

    /// Remove an assembly, returning its staged bytes to the global budget.
    /// Returns the settled (ordinal, logicalFrameId) for tombstoning.
    pub(super) fn release(&mut self, key: &(String, String)) -> Option<(u64, String)> {
        let a = self.assemblies.remove(key)?;
        self.staged_decoded_bytes -= a.decoded_bytes;
        Some((a.ordinal, a.logical_frame_id))
    }

    /// Drop fragment groups stuck for >30s (the protocol's assembly cap) and
    /// return their topics so the caller can resync those routes.
    pub fn sweep_timeouts(&mut self) -> Vec<String> {
        let stale: Vec<(String, String)> = self
            .assemblies
            .iter()
            .filter(|(_, a)| a.started.elapsed() > ASSEMBLY_TIMEOUT)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &stale {
            if let Some((ordinal, id)) = self.release(key) {
                self.settle(key, ordinal, &id);
            }
        }
        stale.into_iter().map(|(topic, _)| topic).collect()
    }

    /// Drop all fragment state for `topics` (connection reset / unsubscribe):
    /// active assemblies and their staged bytes are released. Tombstones for
    /// those routes are cleared too — a fresh subscription re-keys the route.
    pub fn forget_topics(&mut self, topics: &[String]) {
        let doomed: Vec<(String, String)> = self
            .assemblies
            .keys()
            .filter(|(topic, _)| topics.contains(topic))
            .cloned()
            .collect();
        for key in &doomed {
            let _ = self.release(key);
        }
        self.settled.retain(|(topic, _), _| !topics.contains(topic));
    }

    /// Ingest the params of a `v4/conversation/frame` notification.
    /// Returns a logical frame when one fully arrived; Ok(None) for
    /// tombstoned ordinals or incomplete fragment groups. Every Err carries a
    /// canonical reason code; the caller must resync the route (nothing was
    /// applied).
    pub fn ingest(&mut self, params: &Value) -> Result<Option<LogicalFrame>, String> {
        self.sweep_timeouts();
        // Physical budget: the re-serialized envelope must fit maxFrameBytes
        // (mirror of measureTopicNotificationEnvelopeBytes).
        if serde_json::to_vec(params).is_ok_and(|b| b.len() > MAX_FRAME_BYTES) {
            return Err("proto.frameEnvelopeTooLarge: physical frame exceeds 1 MiB".into());
        }
        let Some(obj) = params.as_object() else {
            return Err("proto.frameAssemblyMetadataMismatch: envelope not an object".into());
        };
        if obj.get("wireVersion").and_then(Value::as_u64) != Some(WIRE_VERSION) {
            return Err(format!(
                "proto.frameAssemblyMetadataMismatch: wireVersion != {WIRE_VERSION}"
            ));
        }
        let kind = req_str(obj, "kind")?;
        let topic = req_nonempty(obj, "topic")?;
        let sub = req_nonempty(obj, "subscriptionId")?;
        let id = req_nonempty(obj, "logicalFrameId")?;
        let ordinal = req_u64(obj, "logicalFrameOrdinal")?;
        if ordinal == 0 {
            return Err("proto.frameAssemblyMetadataMismatch: ordinal must be positive".into());
        }
        let delivery = match obj.get("deliveryKind").and_then(Value::as_str) {
            // Distinct arms so each returns a 'static literal, not a borrow
            // of the params.
            Some("initial") => "initial",
            Some("online") => "online",
            Some("recovery") => "recovery",
            _ => {
                self.settle(&(topic.clone(), sub.clone()), ordinal, &id);
                return Err("proto.frameAssemblyMetadataMismatch: invalid deliveryKind".into());
            }
        };
        let key = (topic.clone(), sub.clone());

        // Ordinal tombstone: below → silently stale; equal with a different id
        // → conflict fault; equal with the same id → exact replay, dropped.
        if let Some(&(settled_ord, ref settled_id)) = self.settled.get(&key) {
            if ordinal < settled_ord {
                return Ok(None);
            }
            if ordinal == settled_ord {
                if settled_id.as_str() != id {
                    return Err(format!(
                        "proto.frameAssemblyOrdinalConflict: {ordinal} settled as {settled_id}, got {id}"
                    ));
                }
                return Ok(None);
            }
        }

        match kind {
            "complete" => self.ingest_complete(key, obj, &topic, &sub, &id, ordinal),
            "fragment" => self.ingest_fragment(key, obj, delivery, &topic, &sub, &id, ordinal),
            other => Err(format!(
                "proto.frameAssemblyMetadataMismatch: unknown kind {other}"
            )),
        }
    }

    fn ingest_complete(
        &mut self,
        key: (String, String),
        obj: &serde_json::Map<String, Value>,
        topic: &str,
        sub: &str,
        id: &str,
        ordinal: u64,
    ) -> Result<Option<LogicalFrame>, String> {
        let fault =
            |e: String| -> String { format!("{e} (complete frame {id}, ordinal {ordinal})") };
        let frame = obj
            .get("frame")
            .ok_or_else(|| fault("proto.frameAssemblyMetadataMismatch: missing frame".into()))?;
        // The inner logical frame must agree with the routing envelope.
        if frame.get("topic").and_then(Value::as_str) != Some(topic)
            || frame.get("subscriptionId").and_then(Value::as_str) != Some(sub)
        {
            self.settle(&key, ordinal, id);
            return Err(fault(
                "proto.frameAssemblyMetadataMismatch: frame/envelope route mismatch".into(),
            ));
        }
        // A complete frame supersedes an incomplete fragment group on the
        // route; the old group's seq hole is caught by the route cursor.
        if let Some((old_ord, old_id)) = self.release(&key) {
            self.settle(&key, old_ord, &old_id);
        }
        let logical = match logical_from_value(frame) {
            Some(l) => l,
            None => {
                self.settle(&key, ordinal, id);
                return Err(fault(
                    "proto.frameAssemblyMetadataMismatch: bad logical frame".into(),
                ));
            }
        };
        if logical.to_seq < logical.from_seq {
            self.settle(&key, ordinal, id);
            return Err(fault(
                "proto.frameAssemblyMetadataMismatch: toSeq < fromSeq".into(),
            ));
        }
        self.settle(&key, ordinal, id);
        Ok(Some(logical))
    }
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "wire_fault_tests.rs"]
mod fault_tests;
#[cfg(test)]
#[path = "wire_test_helpers.rs"]
mod helpers;
#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;

#[path = "wire_frag.rs"]
mod wire_frag;
