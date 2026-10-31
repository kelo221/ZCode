use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gcode-file-read-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A real PNG of about `side × side` noise pixels, so it spans several chunks.
fn png(side: u32) -> Vec<u8> {
    let mut seed = 7u32;
    let img = image::RgbImage::from_fn(side, side, |_, _| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let b = seed.to_le_bytes();
        image::Rgb([b[1], b[2], b[3]])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn ask(path: &Path, offset: u64, limit: u64) -> Value {
    let line = json!({"id": 9, "method": FILE_READ_METHOD,
        "params": {"path": path.to_string_lossy(), "offset": offset, "limit": limit}})
    .to_string();
    serde_json::from_str(&parse(&line).expect("file read request").answer()).unwrap()
}

fn error(reply: &Value) -> String {
    reply["error"]["message"].as_str().unwrap_or("").to_string()
}

#[test]
fn other_methods_are_not_intercepted() {
    assert!(parse(r#"{"id":1,"method":"v4/command","params":{}}"#).is_none());
    let mentions = r#"{"id":1,"method":"v4/command","params":{"text":"gcode/file/read"}}"#;
    assert!(parse(mentions).is_none());
}

#[test]
fn a_multi_chunk_image_reads_back_byte_for_byte() {
    let dir = scratch("chunks");
    let path = dir.join("noise.png");
    let bytes = png(700);
    assert!(bytes.len() as u64 > 2 * FILE_READ_CHUNK, "{}", bytes.len());
    std::fs::write(&path, &bytes).unwrap();

    let mut got = Vec::new();
    let mut offset = 0;
    let mut mtime = None;
    loop {
        let r = ask(&path, offset, 10 * 1024 * 1024);
        let result = &r["result"];
        assert_eq!(result["totalBytes"], bytes.len() as u64, "{r}");
        let this_mtime = result["mtimeMs"].as_u64().unwrap();
        assert_eq!(*mtime.get_or_insert(this_mtime), this_mtime);
        let chunk = BASE64_STANDARD
            .decode(result["dataBase64"].as_str().unwrap())
            .unwrap();
        assert!(chunk.len() as u64 <= FILE_READ_CHUNK);
        got.extend_from_slice(&chunk);
        match result["nextOffset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(got, bytes);
}

#[test]
fn non_images_and_unsafe_requests_are_refused() {
    let dir = scratch("refuse");
    let text = dir.join("notes.txt");
    std::fs::write(&text, "hello").unwrap();
    assert!(error(&ask(&text, 0, 100)).contains("not an image"));

    let fake = dir.join("fake.png");
    std::fs::write(&fake, "not really a png at all").unwrap();
    assert!(error(&ask(&fake, 0, 100)).contains("not the image"));

    let jpeg_named_png = dir.join("photo.jpg");
    std::fs::write(&jpeg_named_png, png(4)).unwrap();
    assert!(error(&ask(&jpeg_named_png, 0, 100)).contains("not the image"));

    let rel = Path::new("relative/shot.png");
    assert!(error(&ask(rel, 0, 100)).contains("absolute"));

    let folder = dir.join("folder.png");
    std::fs::create_dir_all(&folder).unwrap();
    assert!(!error(&ask(&folder, 0, 100)).is_empty());

    let big = dir.join("big.png");
    let file = std::fs::File::create(&big).unwrap();
    file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
    assert!(error(&ask(&big, 0, 100)).contains("too large"));

    let ok = dir.join("ok.png");
    std::fs::write(&ok, png(4)).unwrap();
    assert!(error(&ask(&ok, 0, 0)).contains("limit"));
    assert!(error(&ask(&ok, 1 << 30, 10)).contains("offset"));
    assert!(ask(&ok, 0, 10)["result"]["nextOffset"].as_u64() == Some(10));
}
