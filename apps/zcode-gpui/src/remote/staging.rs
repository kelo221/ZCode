//! Desktop staging for phone attachments (GCode-only, not a CLI method).
//!
//! The phone uploads a file with the same begin/chunk/commit/abort shapes as
//! `v4/attachment/*`, but under `gcode/stage/`. The gateway answers these
//! itself: the bytes are written to a desktop paste temp file and `commit`
//! returns its path, which the phone then sends as a plain attachment
//! reference. This works for a new-task draft, where the CLI's own upload
//! cannot (it needs a session id), and the agent reads the file as if it had
//! been pasted on the desktop. Files are not deleted when the phone removes
//! them (it may already have sent them); the paste-dir sweep reclaims them.

use crate::composer::attachment::{
    ATTACHMENT_CHUNK_MAX_BYTES, ATTACHMENT_MAX_BYTES, ATTACHMENT_MAX_CHUNKS, calculate_sha256,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(crate) const STAGE_PREFIX: &str = "gcode/stage/";
/// Per phone socket, matching the client's own staging caps.
const MAX_OPEN_UPLOADS: usize = 16;
const MAX_OPEN_BYTES: usize = 64 * 1024 * 1024;

struct Stage {
    file_name: String,
    total_bytes: usize,
    total_chunks: usize,
    checksum: String,
    data: Vec<u8>,
    next_chunk: usize,
}

/// Uploads in progress on one phone socket; dropped with the socket.
#[derive(Default)]
pub(crate) struct StagedUploads {
    uploads: HashMap<String, Stage>,
}

type Reply = Result<Value, &'static str>;

impl StagedUploads {
    /// Answer `line` when it is a `gcode/stage/*` request; `None` otherwise,
    /// so the line goes through the normal phone scope.
    pub(crate) fn intercept(&mut self, line: &str) -> Option<String> {
        if !line.contains(STAGE_PREFIX) {
            return None;
        }
        let Ok(Value::Object(msg)) = serde_json::from_str::<Value>(line) else {
            return None;
        };
        let step = msg
            .get("method")
            .and_then(Value::as_str)?
            .strip_prefix(STAGE_PREFIX)?
            .to_string();
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let empty = Map::new();
        let params = msg
            .get("params")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let reply = match step.as_str() {
            "begin" => self.begin(params),
            "chunk" => self.chunk(params),
            "commit" => self.commit(params),
            "abort" => {
                self.uploads.remove(text(params, "uploadId"));
                Ok(json!({}))
            }
            _ => Err("unknown staging method"),
        };
        Some(
            match reply {
                Ok(result) => json!({"id": id, "result": result}),
                Err(message) => json!({"id": id, "error": {"code": -32602, "message": message}}),
            }
            .to_string(),
        )
    }

    fn begin(&mut self, params: &Map<String, Value>) -> Reply {
        let upload_id = text(params, "uploadId");
        let total_bytes = number(params, "totalBytes")?;
        let total_chunks = number(params, "totalChunks")?;
        let open: usize = self.uploads.values().map(|s| s.total_bytes).sum();
        if upload_id.is_empty() || self.uploads.contains_key(upload_id) {
            return Err("invalid upload id");
        }
        if total_bytes > ATTACHMENT_MAX_BYTES
            || total_chunks > ATTACHMENT_MAX_CHUNKS
            || (total_bytes == 0) != (total_chunks == 0)
            || total_bytes > total_chunks * ATTACHMENT_CHUNK_MAX_BYTES
        {
            return Err("attachment too large");
        }
        if self.uploads.len() >= MAX_OPEN_UPLOADS || open + total_bytes > MAX_OPEN_BYTES {
            return Err("too many attachments in progress");
        }
        self.uploads.insert(
            upload_id.to_string(),
            Stage {
                file_name: text(params, "fileName").to_string(),
                total_bytes,
                total_chunks,
                checksum: text(params, "checksum").to_string(),
                data: Vec::with_capacity(total_bytes),
                next_chunk: 0,
            },
        );
        Ok(json!({"state": "staging", "uploadId": upload_id, "nextChunkIndex": 0}))
    }

    fn chunk(&mut self, params: &Map<String, Value>) -> Reply {
        let upload_id = text(params, "uploadId");
        let index = number(params, "chunkIndex")?;
        let stage = self.uploads.get_mut(upload_id).ok_or("unknown upload")?;
        if index != stage.next_chunk || index >= stage.total_chunks {
            return Err("chunk out of order");
        }
        let bytes = BASE64_STANDARD
            .decode(text(params, "dataBase64"))
            .map_err(|_| "invalid chunk data")?;
        if bytes.is_empty()
            || bytes.len() > ATTACHMENT_CHUNK_MAX_BYTES
            || stage.data.len() + bytes.len() > stage.total_bytes
        {
            return Err("invalid chunk size");
        }
        stage.data.extend_from_slice(&bytes);
        stage.next_chunk += 1;
        Ok(json!({"uploadId": upload_id, "nextChunkIndex": stage.next_chunk}))
    }

    fn commit(&mut self, params: &Map<String, Value>) -> Reply {
        let stage = self
            .uploads
            .remove(text(params, "uploadId"))
            .ok_or("unknown upload")?;
        if stage.next_chunk != stage.total_chunks
            || stage.data.len() != stage.total_bytes
            || calculate_sha256(&stage.data) != stage.checksum
        {
            return Err("attachment checksum mismatch");
        }
        let path = crate::shared::temp_attachments::write_temp_image(
            &stage.data,
            &extension(&stage.file_name),
        )
        .map_err(|_| "could not save the attachment on the desktop")?;
        Ok(json!({"ref": path.to_string_lossy()}))
    }
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params.get(key).and_then(Value::as_str).unwrap_or("")
}

fn number(params: &Map<String, Value>, key: &str) -> Result<usize, &'static str> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("invalid size")
}

/// The phone's file extension, kept so the agent sees the right file type.
/// Anything unusual becomes `bin`; the name never reaches the path otherwise.
fn extension(file_name: &str) -> String {
    std::path::Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty() && ext.len() <= 10)
        .filter(|ext| ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .map_or_else(|| "bin".to_string(), str::to_ascii_lowercase)
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
