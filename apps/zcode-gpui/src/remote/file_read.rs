//! `gcode/file/read`: the phone fetches a desktop image file it was shown
//! (a Markdown image embed or a Read-tool image) in chunks. GCode-only; the
//! gateway answers it and it never reaches the CLI.
//!
//! Only image files are served: an absolute path whose extension is an image
//! type and whose first bytes are that format, at most `MAX_IMAGE_BYTES`.
//! Each answer carries the file's size and mtime so the phone can reject a
//! file that changed between chunks.

use crate::files::image_preview::{MAX_IMAGE_BYTES, format};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde_json::{Value, json};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub(crate) const FILE_READ_METHOD: &str = "gcode/file/read";
/// Decoded bytes per chunk; base64 keeps the frame well under 1 MiB.
pub(crate) const FILE_READ_CHUNK: u64 = 384 * 1024;

pub(crate) struct FileRead {
    id: Value,
    path: String,
    offset: u64,
    limit: u64,
}

/// The request in `line` when it is a `gcode/file/read`; `None` otherwise.
pub(crate) fn parse(line: &str) -> Option<FileRead> {
    if !line.contains(FILE_READ_METHOD) {
        return None;
    }
    let Ok(Value::Object(msg)) = serde_json::from_str::<Value>(line) else {
        return None;
    };
    if msg.get("method").and_then(Value::as_str) != Some(FILE_READ_METHOD) {
        return None;
    }
    let params = msg.get("params");
    let field = |key: &str| params.and_then(|p| p.get(key));
    Some(FileRead {
        id: msg.get("id").cloned().unwrap_or(Value::Null),
        path: field("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        offset: field("offset").and_then(Value::as_u64).unwrap_or(u64::MAX),
        limit: field("limit").and_then(Value::as_u64).unwrap_or(0),
    })
}

impl FileRead {
    /// The response line. Blocking file IO; run it off the UI thread.
    pub(crate) fn answer(&self) -> String {
        match read(Path::new(&self.path), self.offset, self.limit) {
            Ok(result) => json!({"id": self.id, "result": result}),
            Err(message) => json!({"id": self.id, "error": {"code": -32602, "message": message}}),
        }
        .to_string()
    }
}

fn read(path: &Path, offset: u64, limit: u64) -> Result<Value, &'static str> {
    if !path.is_absolute() {
        return Err("path must be absolute");
    }
    let expected = format(path).ok_or("not an image file")?;
    if limit == 0 {
        return Err("invalid limit");
    }
    let mut file = std::fs::File::open(path).map_err(|_| "file is unavailable")?;
    let meta = file.metadata().map_err(|_| "file is unavailable")?;
    if !meta.is_file() {
        return Err("not a regular file");
    }
    let total = meta.len();
    if total == 0 || total > MAX_IMAGE_BYTES {
        return Err("image is empty or too large");
    }
    if offset > total {
        return Err("offset out of range");
    }
    let mut head = Vec::with_capacity(64);
    (&mut file)
        .take(64)
        .read_to_end(&mut head)
        .map_err(|_| "read failed")?;
    if image::guess_format(&head).ok() != Some(expected) {
        return Err("file content is not the image its name says");
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| "read failed")?;
    let mut data = Vec::new();
    file.take(limit.min(FILE_READ_CHUNK).min(total - offset))
        .read_to_end(&mut data)
        .map_err(|_| "read failed")?;
    let end = offset + data.len() as u64;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    Ok(json!({
        "dataBase64": BASE64_STANDARD.encode(&data),
        "totalBytes": total,
        "mtimeMs": mtime_ms,
        "nextOffset": (end < total && !data.is_empty()).then_some(end),
    }))
}

#[cfg(test)]
#[path = "file_read_tests.rs"]
mod tests;
