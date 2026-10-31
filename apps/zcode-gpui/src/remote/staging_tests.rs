use super::*;
use crate::composer::attachment::{build_begin_params, build_chunk_params, chunk_payload};

fn request(id: u64, step: &str, params: Value) -> String {
    json!({"id": id, "method": format!("{STAGE_PREFIX}{step}"), "params": params}).to_string()
}

fn reply(staging: &mut StagedUploads, line: String) -> Value {
    serde_json::from_str(&staging.intercept(&line).expect("staging request")).unwrap()
}

fn stage(staging: &mut StagedUploads, upload: &str, name: &str, bytes: &[u8]) -> Value {
    let chunks = chunk_payload(bytes).unwrap();
    let begin = build_begin_params(
        "c",
        "",
        upload,
        name,
        "text/plain",
        bytes.len(),
        chunks.len(),
        &calculate_sha256(bytes),
    );
    let r = reply(staging, request(1, "begin", begin));
    assert_eq!(r["result"]["state"], "staging", "{r}");
    for (index, chunk) in chunks.iter().enumerate() {
        let params = build_chunk_params("c", "", upload, index, chunk);
        let r = reply(staging, request(2, "chunk", params));
        assert_eq!(r["result"]["nextChunkIndex"], index + 1, "{r}");
    }
    reply(staging, request(3, "commit", json!({"uploadId": upload})))
}

#[test]
fn other_lines_pass_through() {
    let mut staging = StagedUploads::default();
    let line = json!({"id": 1, "method": "v4/command", "params": {}}).to_string();
    assert_eq!(staging.intercept(&line), None);
}

#[test]
fn committed_upload_is_a_desktop_file_with_the_same_bytes() {
    let mut staging = StagedUploads::default();
    let bytes: Vec<u8> = (0..900_000u32).map(|i| (i % 251) as u8).collect();
    let r = stage(&mut staging, "u1", "notes.MD", &bytes);
    let path = std::path::PathBuf::from(r["result"]["ref"].as_str().unwrap());
    assert!(path.is_absolute());
    assert_eq!(path.extension().unwrap(), "md");
    assert!(crate::shared::temp_attachments::is_owned_path(
        &path.to_string_lossy()
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(crate::backend::attachment_file::valid_staged_path(
        &path.to_string_lossy()
    ));
    crate::shared::temp_attachments::delete_owned(&path);
}

#[test]
fn checksum_mismatch_and_bad_order_are_refused() {
    let mut staging = StagedUploads::default();
    let begin = build_begin_params("c", "", "u", "a.txt", "text/plain", 3, 1, "sha256:00");
    reply(&mut staging, request(1, "begin", begin));
    let wrong = build_chunk_params("c", "", "u", 1, "YWJj");
    assert!(reply(&mut staging, request(2, "chunk", wrong))["error"].is_object());
    let chunk = build_chunk_params("c", "", "u", 0, "YWJj");
    assert!(reply(&mut staging, request(3, "chunk", chunk))["result"].is_object());
    let r = reply(&mut staging, request(4, "commit", json!({"uploadId": "u"})));
    assert_eq!(r["error"]["message"], "attachment checksum mismatch");
}

#[test]
fn oversized_uploads_are_refused_before_any_bytes_arrive() {
    let mut staging = StagedUploads::default();
    let begin = build_begin_params(
        "c",
        "",
        "u",
        "big.bin",
        "application/octet-stream",
        ATTACHMENT_MAX_BYTES + 1,
        ATTACHMENT_MAX_CHUNKS,
        "sha256:00",
    );
    assert!(reply(&mut staging, request(1, "begin", begin))["error"].is_object());
}

#[test]
fn file_extensions_never_shape_the_path() {
    assert_eq!(extension("photo.JPG"), "jpg");
    assert_eq!(extension("../../evil"), "bin");
    assert_eq!(extension("a.b/c"), "bin");
    assert_eq!(extension("x.tar\\..\\y"), "bin");
    assert_eq!(extension("noext"), "bin");
}
