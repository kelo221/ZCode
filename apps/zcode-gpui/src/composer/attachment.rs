//! Client-side chunked attachment upload transactions.
//!
//! Protocol spec: packages/shared/src/zcode-protocol-v4/transport.ts
//! and packages/ui/src/v4/attachmentUploadTransaction.ts.
//!
//! Methods:
//! - v4/attachment/begin
//! - v4/attachment/chunk
//! - v4/attachment/commit
//! - v4/attachment/abort

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// 384 KiB is divisible by 3, so every intermediate chunk base64 has no padding
/// and easily fits inside the 1 MiB physical NDJSON frame limit.
pub const ATTACHMENT_UPLOAD_CHUNK_BYTES: usize = 384 * 1024;

/// `PROTOCOL_V4_LIMITS.attachmentChunkMaxBytes` (decoded bytes per chunk).
pub const ATTACHMENT_CHUNK_MAX_BYTES: usize = 512 * 1024;

/// `PROTOCOL_V4_LIMITS.attachmentMaxBytes` (20 MiB).
pub const ATTACHMENT_MAX_BYTES: usize = 20 * 1024 * 1024;

/// `PROTOCOL_V4_LIMITS.attachmentUploadMaxChunks`.
pub const ATTACHMENT_MAX_CHUNKS: usize = 64;

/// The only upload methods this client may call. `v4/attachment/put` is
/// internal to the host and deliberately absent.
#[cfg(test)]
pub const ATTACHMENT_UPLOAD_METHODS: [&str; 4] = [
    "v4/attachment/begin",
    "v4/attachment/chunk",
    "v4/attachment/commit",
    "v4/attachment/abort",
];

const _: () = assert!(ATTACHMENT_UPLOAD_CHUNK_BYTES <= ATTACHMENT_CHUNK_MAX_BYTES);
const _: () =
    assert!(ATTACHMENT_MAX_BYTES.div_ceil(ATTACHMENT_UPLOAD_CHUNK_BYTES) <= ATTACHMENT_MAX_CHUNKS);

/// Attachment reference attached to a user turn or message submission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub file_name: String,
    pub mime: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_ref: Option<String>,
}

/// Calculate the `sha256:<hex>` digest required by `attachmentBeginV4`.
pub fn calculate_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{:02x}", b);
    }
    format!("sha256:{}", hex)
}

/// Split raw file bytes into base64-encoded chunks of at most 384 KiB each.
/// A zero-byte file has zero chunks: `begin` requires
/// `totalBytes == 0 <=> totalChunks == 0`, and the CLI rejects empty chunks
/// (`fault.attachment.emptyChunk`).
pub fn chunk_payload(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() > ATTACHMENT_MAX_BYTES {
        return Err(format!(
            "File size {} exceeds maximum allowed {} bytes",
            bytes.len(),
            ATTACHMENT_MAX_BYTES
        ));
    }
    Ok(bytes
        .chunks(ATTACHMENT_UPLOAD_CHUNK_BYTES)
        .map(|slice| BASE64_STANDARD.encode(slice))
        .collect())
}

/// Params for `v4/attachment/begin` (`v4AttachmentBeginParamsSchema`, strict:
/// no workspace fields; the stdio connection already scopes the workspace).
#[allow(clippy::too_many_arguments)]
pub fn build_begin_params(
    connection_id: &str,
    session_id: &str,
    upload_id: &str,
    file_name: &str,
    mime: &str,
    total_bytes: usize,
    total_chunks: usize,
    checksum: &str,
) -> Value {
    json!({
        "connectionId": connection_id,
        "uploadId": upload_id,
        "sessionId": session_id,
        "fileName": file_name,
        "mime": mime,
        "totalBytes": total_bytes,
        "totalChunks": total_chunks,
        "checksum": checksum,
    })
}

/// Params for `v4/attachment/chunk` (`v4AttachmentChunkParamsSchema`, strict).
pub fn build_chunk_params(
    connection_id: &str,
    session_id: &str,
    upload_id: &str,
    chunk_index: usize,
    data_base64: &str,
) -> Value {
    json!({
        "connectionId": connection_id,
        "uploadId": upload_id,
        "sessionId": session_id,
        "chunkIndex": chunk_index,
        "dataBase64": data_base64,
    })
}

/// Params for `v4/attachment/commit` or `v4/attachment/abort` (strict).
pub fn build_terminal_params(connection_id: &str, session_id: &str, upload_id: &str) -> Value {
    json!({
        "connectionId": connection_id,
        "uploadId": upload_id,
        "sessionId": session_id,
    })
}

/// Generate a unique upload transaction ID.
pub fn generate_upload_id() -> String {
    format!("upload-{}", uuid::Uuid::now_v7())
}

/// Detect basic mime type from file name extension.
pub fn detect_mime_type(file_name: &str) -> &'static str {
    let ext = file_name
        .rsplit('.')
        .next()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "rs" => "text/x-rust",
        "ts" | "tsx" => "text/typescript",
        "js" | "jsx" => "text/javascript",
        "html" => "text/html",
        "css" => "text/css",
        _ => "application/octet-stream",
    }
}

/// Format human-readable byte sizes (e.g. 42 KB, 1.2 MB).
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
#[path = "attachment_tests.rs"]
mod tests;
