//! Unit tests for chunked attachment uploads.

use super::*;

#[test]
fn test_calculate_sha256() {
    let data = b"hello zcode";
    let hash = calculate_sha256(data);
    assert!(hash.starts_with("sha256:"));
    // sha256("hello zcode") = 8ef59664...
    assert_eq!(hash.len(), 7 + 64);
}

#[test]
fn test_chunking_small_data() {
    let data = b"small payload";
    let chunks = chunk_payload(data).unwrap();
    assert_eq!(chunks.len(), 1);
    let decoded = BASE64_STANDARD.decode(&chunks[0]).unwrap();
    assert_eq!(decoded, data);
}

#[test]
fn test_chunking_oversized_error() {
    let large = vec![0u8; ATTACHMENT_MAX_BYTES + 1];
    let err = chunk_payload(&large);
    assert!(err.is_err());
}

#[test]
fn test_chunking_multichunk() {
    // 384 KiB + 10 bytes -> 2 chunks
    let len = ATTACHMENT_UPLOAD_CHUNK_BYTES + 10;
    let data = vec![42u8; len];
    let chunks = chunk_payload(&data).unwrap();
    assert_eq!(chunks.len(), 2);
    let d1 = BASE64_STANDARD.decode(&chunks[0]).unwrap();
    let d2 = BASE64_STANDARD.decode(&chunks[1]).unwrap();
    assert_eq!(d1.len(), ATTACHMENT_UPLOAD_CHUNK_BYTES);
    assert_eq!(d2.len(), 10);
}

#[test]
fn test_params_builders_match_strict_schemas() {
    let keys = |v: &Value| {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let begin = build_begin_params(
        "conn-1",
        "s1",
        "up-1",
        "foo.png",
        "image/png",
        100,
        1,
        "sha256:abc",
    );
    assert_eq!(
        keys(&begin),
        [
            "checksum",
            "connectionId",
            "fileName",
            "mime",
            "sessionId",
            "totalBytes",
            "totalChunks",
            "uploadId"
        ]
    );
    assert_eq!(begin["connectionId"], "conn-1");

    let chunk = build_chunk_params("conn-1", "s1", "up-1", 0, "YmFzZTY0");
    assert_eq!(
        keys(&chunk),
        [
            "chunkIndex",
            "connectionId",
            "dataBase64",
            "sessionId",
            "uploadId"
        ]
    );

    let term = build_terminal_params("conn-1", "s1", "up-1");
    assert_eq!(keys(&term), ["connectionId", "sessionId", "uploadId"]);
}

#[test]
fn test_empty_file_declares_zero_chunks() {
    assert!(chunk_payload(&[]).unwrap().is_empty());
}

#[test]
fn test_max_payload_chunk_and_total_limits() {
    let data: Vec<u8> = (0..ATTACHMENT_MAX_BYTES).map(|i| (i % 251) as u8).collect();
    let chunks = chunk_payload(&data).unwrap();
    assert!(chunks.len() <= ATTACHMENT_MAX_CHUNKS);
    let mut joined = Vec::with_capacity(data.len());
    for c in &chunks {
        let decoded = BASE64_STANDARD.decode(c).unwrap();
        assert!(!decoded.is_empty() && decoded.len() <= ATTACHMENT_CHUNK_MAX_BYTES);
        joined.extend_from_slice(&decoded);
    }
    // The begin checksum covers exactly the concatenation of the chunks.
    assert_eq!(calculate_sha256(&joined), calculate_sha256(&data));
    assert_eq!(joined, data);
}

#[test]
fn test_sha256_known_vector_and_format() {
    assert_eq!(
        calculate_sha256(b"abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_internal_put_method_is_never_used() {
    assert!(!ATTACHMENT_UPLOAD_METHODS.contains(&"v4/attachment/put"));
    for src in [
        include_str!("attachment.rs"),
        include_str!("tray.rs"),
        include_str!("../backend/commands.rs"),
    ] {
        // A call needs the method as a string literal; doc comments use backticks.
        assert!(!src.contains(concat!("\"v4/attachment", "/put\"")));
    }
}

#[test]
fn test_mime_detection() {
    assert_eq!(detect_mime_type("photo.png"), "image/png");
    assert_eq!(detect_mime_type("file.ts"), "text/typescript");
    assert_eq!(detect_mime_type("unknown.bin"), "application/octet-stream");
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(500), "500 B");
    assert_eq!(format_bytes(2048), "2.0 KB");
    assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
}

#[test]
fn test_check_gpui_path_prompt() {
    let opt = gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: None,
    };
    assert!(opt.files);
}
