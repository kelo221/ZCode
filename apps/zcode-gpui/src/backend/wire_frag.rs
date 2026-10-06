//! Fragment assembly half of the wire assembler (split from backend/wire.rs
//! for the 400-line cap). Mirrors the fragment branch of the canonical
//! TypeScript `TopicWireFrameAssembler`.

use super::{Assembly, FrameAssembler};
use base64::Engine;
use serde_json::Value;

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

impl FrameAssembler {
    /// Fragment branch of `ingest`. Preconditions enforced by the caller:
    /// wireVersion, kind, route fields, ordinal and the ordinal tombstone.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ingest_fragment(
        &mut self,
        key: (String, String),
        obj: &serde_json::Map<String, Value>,
        delivery: &'static str,
        topic: &str,
        sub: &str,
        id: &str,
        ordinal: u64,
    ) -> Result<Option<super::LogicalFrame>, String> {
        let fault = |e: String| -> String {
            format!("{e} (fragment frame {id}, ordinal {ordinal}, topic {topic})")
        };
        let bad = "proto.frameAssemblyMetadataMismatch: bad fragment fields".to_string();
        let index = req_u64(obj, "fragmentIndex").map_err(&fault)?;
        let count = req_u64(obj, "fragmentCount").map_err(&fault)?;
        let logical_bytes = req_u64(obj, "logicalBytes").map_err(&fault)?;
        let data_b64 = req_str(obj, "dataBase64").map_err(&fault)?;
        let checksum = {
            let c = obj.get("checksum").ok_or_else(|| fault(bad.clone()))?;
            let algo = c.get("algorithm").and_then(Value::as_str).unwrap_or("");
            let value = c.get("value").and_then(Value::as_str).unwrap_or("");
            // Canonical schema requires LOWERCASE hex: uppercase is a
            // fault, not a normalizable variant (review finding 5).
            if algo != "crc32"
                || value.len() != 8
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                self.settle(&key, ordinal, id);
                return Err(fault(bad));
            }
            value.to_string()
        };
        // Hard limits before any allocation mirrors the canonical order.
        if count < 1 || count > super::MAX_FRAGMENTS as u64 {
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameFragmentCountExceeded".into()));
        }
        if logical_bytes < 1 || count > logical_bytes {
            self.settle(&key, ordinal, id);
            return Err(fault(bad));
        }
        if logical_bytes > super::MAX_ASSEMBLY_BYTES as u64 {
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameAssemblyTooLarge".into()));
        }
        if index >= count {
            self.settle(&key, ordinal, id);
            return Err(fault(bad));
        }
        // Strict padded standard base64 (wire-binary.ts topicWireBase64Schema).
        if data_b64.len() < 4 || data_b64.len() % 4 != 0 {
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameAssemblyInvalidBase64".into()));
        }
        let chunk = match base64::engine::general_purpose::STANDARD.decode(data_b64) {
            Ok(c) => c,
            Err(_) => {
                self.settle(&key, ordinal, id);
                return Err(fault("proto.frameAssemblyInvalidBase64".into()));
            }
        };
        let chunk_len = chunk.len();

        // Superseded / conflicting assembly for the same route. State 3 is
        // the normal continuation of the live group (same ordinal + id).
        let state = self.assemblies.get(&key).map(|cur| {
            if ordinal < cur.ordinal {
                0 // older: silently stale
            } else if ordinal == cur.ordinal && cur.logical_frame_id != id {
                1 // conflict at the live ordinal
            } else if ordinal > cur.ordinal {
                2 // higher ordinal supersedes the incomplete group
            } else {
                3 // same ordinal, same id: continue the group
            }
        });
        match state {
            Some(0) => return Ok(None),
            Some(1) => {
                self.release(&key);
                self.settle(&key, ordinal, id);
                return Err(fault("proto.frameAssemblyOrdinalConflict".into()));
            }
            Some(2) => {
                // Release the superseded group: its seq hole is caught by the
                // route cursor's gap check.
                self.release(&key);
            }
            _ => {}
        }

        // Metadata equality for every fragment of one assembly.
        let metadata_conflict = self.assemblies.get(&key).is_some_and(|cur| {
            cur.fragment_count != count as usize
                || cur.delivery_kind != delivery
                || cur.logical_bytes != logical_bytes as usize
                || cur.checksum != checksum
        });
        if metadata_conflict {
            self.release(&key);
            self.settle(&key, ordinal, id);
            return Err(fault(
                "proto.frameAssemblyMetadataMismatch: fragment metadata conflict".into(),
            ));
        }
        if !self.assemblies.contains_key(&key) {
            if self.assemblies.len() >= super::MAX_CONCURRENT_ASSEMBLIES {
                self.settle(&key, ordinal, id);
                return Err(fault("proto.frameAssemblyConcurrentLimit".into()));
            }
            self.assemblies.insert(
                key.clone(),
                Assembly {
                    ordinal,
                    logical_frame_id: id.to_string(),
                    delivery_kind: delivery,
                    fragment_count: count as usize,
                    logical_bytes: logical_bytes as usize,
                    checksum,
                    fragments: vec![None; count as usize],
                    received: 0,
                    decoded_bytes: 0,
                    started: std::time::Instant::now(),
                },
            );
        }

        // Duplicate fragment with different bytes is a conflict; equal bytes
        // are a benign replay. Both end the fragment checks here.
        let dup = self
            .assemblies
            .get(&key)
            .and_then(|cur| cur.fragments[index as usize].as_ref())
            .map(|prev| prev != &chunk);
        if dup == Some(true) {
            self.release(&key);
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameAssemblyFragmentConflict".into()));
        }
        if dup.is_some() {
            return Ok(None);
        }
        // Global budget before EVERY newly staged fragment (review finding
        // 4 — this is the real 32 MiB bound, not the creation-time check).
        if self.staged_decoded_bytes.saturating_add(chunk_len) > self.staged_budget {
            self.release(&key);
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameAssemblyBudgetExceeded".into()));
        }
        let overflows = self
            .assemblies
            .get(&key)
            .is_some_and(|cur| cur.decoded_bytes + chunk_len > cur.logical_bytes);
        if overflows {
            self.release(&key);
            self.settle(&key, ordinal, id);
            return Err(fault("proto.frameAssemblyLengthMismatch".into()));
        }
        let complete = {
            let cur = self.assemblies.get_mut(&key).expect("assembly exists");
            cur.fragments[index as usize] = Some(chunk);
            cur.received += 1;
            cur.decoded_bytes += chunk_len;
            cur.received == cur.fragment_count
        };
        self.staged_decoded_bytes += chunk_len;
        if !complete {
            return Ok(None);
        }
        self.finish_fragment(&key, ordinal, id, topic, sub)
    }

    /// A fragment group reached its declared count: verify the joined logical
    /// frame (length, CRC-32, UTF-8, JSON, envelope agreement) and emit it.
    fn finish_fragment(
        &mut self,
        key: &(String, String),
        ordinal: u64,
        id: &str,
        topic: &str,
        sub: &str,
    ) -> Result<Option<super::LogicalFrame>, String> {
        let fault =
            |e: String| -> String { format!("{e} (fragment frame {id}, ordinal {ordinal})") };
        let (checksum, bytes) = {
            let cur = self.assemblies.remove(key).expect("assembly exists");
            self.staged_decoded_bytes -= cur.decoded_bytes;
            if cur.decoded_bytes != cur.logical_bytes {
                self.settle(key, ordinal, id);
                return Err(fault("proto.frameAssemblyLengthMismatch".into()));
            }
            (
                cur.checksum,
                cur.fragments
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect::<Vec<u8>>(),
            )
        };
        if format!("{:08x}", crc32fast::hash(&bytes)) != checksum {
            self.settle(key, ordinal, id);
            return Err(fault("proto.frameAssemblyChecksumMismatch".into()));
        }
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                self.settle(key, ordinal, id);
                return Err(fault("proto.frameAssemblyInvalidUtf8".into()));
            }
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                self.settle(key, ordinal, id);
                return Err(fault("proto.frameAssemblyInvalidJson".into()));
            }
        };
        let logical = match super::logical_from_value(&value) {
            Some(l) => l,
            None => {
                self.settle(key, ordinal, id);
                return Err(fault(
                    "proto.frameAssemblyMetadataMismatch: bad logical frame".into(),
                ));
            }
        };
        // Same seq-shape rule as complete frames.
        if logical.to_seq < logical.from_seq {
            self.settle(key, ordinal, id);
            return Err(fault(
                "proto.frameAssemblyMetadataMismatch: toSeq < fromSeq".into(),
            ));
        }
        if logical.topic != topic || logical.subscription_id != sub {
            self.settle(key, ordinal, id);
            return Err(fault(
                "proto.frameAssemblyMetadataMismatch: frame/envelope route mismatch".into(),
            ));
        }
        self.settle(key, ordinal, id);
        Ok(Some(logical))
    }

    /// Complete-frame branch of `ingest`. Same preconditions as
    /// `ingest_fragment` (tombstone already applied by the caller).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ingest_complete(
        &mut self,
        key: (String, String),
        obj: &serde_json::Map<String, Value>,
        topic: &str,
        sub: &str,
        id: &str,
        ordinal: u64,
    ) -> Result<Option<super::LogicalFrame>, String> {
        let fault =
            |e: String| -> String { format!("{e} (complete frame {id}, ordinal {ordinal})") };
        let frame = obj
            .get("frame")
            .ok_or_else(|| fault("proto.frameAssemblyMetadataMismatch: missing frame".into()))?;
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
        let logical = match super::logical_from_value(frame) {
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
