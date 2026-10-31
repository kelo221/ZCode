use super::*;
use crate::remote::scope::PHONE_SCOPE_REFUSAL;
use gpui::{AppContext, TestAppContext};
use std::sync::mpsc::{Receiver, channel};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gcode-dimg-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn chunk(data: &[u8], total: u64, stamp: u64, next: Option<u64>) -> Chunk {
    Chunk {
        data: data.to_vec(),
        total,
        stamp,
        next,
    }
}

#[test]
fn chunks_join_and_a_file_changed_midway_is_rejected() {
    let mut a = Assembly::default();
    assert_eq!(a.push(chunk(b"abc", 5, 1, Some(3))), Ok(Some(3)));
    assert_eq!(a.push(chunk(b"de", 5, 1, None)), Ok(None));
    assert_eq!(a.bytes, b"abcde");

    let mut changed = Assembly::default();
    changed.push(chunk(b"abc", 5, 1, Some(3))).unwrap();
    assert!(changed.push(chunk(b"de", 5, 2, None)).is_err());

    let mut short = Assembly::default();
    assert!(short.push(chunk(b"abc", 5, 1, None)).is_err());
    let mut skipped = Assembly::default();
    assert!(skipped.push(chunk(b"abc", 5, 1, Some(4))).is_err());
}

#[test]
fn trim_drops_the_oldest_files_first() {
    let dir = scratch("trim");
    for (i, name) in ["old", "mid", "new"].iter().enumerate() {
        std::fs::write(dir.join(name), [0u8; 10]).unwrap();
        let f = std::fs::File::options()
            .write(true)
            .open(dir.join(name))
            .unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i as u64))
            .unwrap();
    }
    trim(&dir, 20);
    assert!(!dir.join("old").exists());
    assert!(dir.join("mid").exists() && dir.join("new").exists());
}

fn phone_state(cx: &mut TestAppContext) -> (gpui::Entity<AppState>, String, Receiver<String>) {
    let state = cx.new(AppState::for_test);
    let (tx, rx) = channel();
    let key = state.update(cx, |s, _| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.phone_client.enabled = true;
        s.workspaces[0].key.clone()
    });
    (state, key, rx)
}

/// Answer every request the phone sent the way the desktop gateway would.
fn answer_as_desktop(
    state: &gpui::Entity<AppState>,
    key: &str,
    rx: &Receiver<String>,
    cx: &mut TestAppContext,
) -> usize {
    let mut answered = 0;
    loop {
        cx.run_until_parked();
        let Ok(line) = rx.try_recv() else {
            return answered;
        };
        let request: Value = serde_json::from_str(&line).unwrap();
        let reply: Value = serde_json::from_str(
            &crate::remote::file_read::parse(&line)
                .expect("phone asks with gcode/file/read")
                .answer(),
        )
        .unwrap();
        state.update(cx, |s, cx| {
            s.handle_response(
                key,
                request["id"].as_u64().unwrap(),
                reply.get("result").cloned(),
                reply.get("error").cloned(),
                cx,
            )
        });
        answered += 1;
    }
}

#[gpui::test]
fn the_phone_fetches_a_desktop_image_once_then_serves_it_from_disk(cx: &mut TestAppContext) {
    let dir = scratch("fetch");
    let cache = dir.join("cache");
    let path = dir.join("shot.png");
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_fn(600, 600, |x, y| {
        image::Rgb([x as u8, y as u8, (x ^ y) as u8])
    })
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let bytes = png.into_inner();
    std::fs::write(&path, &bytes).unwrap();
    let (state, key, rx) = phone_state(cx);

    let run = |cx: &mut TestAppContext| {
        let (weak, key, path, cache) = (
            state.downgrade(),
            key.clone(),
            path.clone(),
            Some(cache.clone()),
        );
        cx.spawn(async move |mut cx| {
            fetch(weak, key, RemoteImage::DesktopFile(path), cache, &mut cx).await
        })
    };

    let first = run(cx);
    let requests = answer_as_desktop(&state, &key, &rx, cx);
    assert_eq!(cx.foreground_executor().block_test(first).unwrap(), bytes);
    assert!(requests >= 1);
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);

    // Only the first chunk is asked for: it carries mtime and size, and the
    // cached copy matching them is used instead of the rest.
    let second = run(cx);
    assert_eq!(answer_as_desktop(&state, &key, &rx, cx), 1);
    assert_eq!(cx.foreground_executor().block_test(second).unwrap(), bytes);
}

#[gpui::test]
fn a_cached_attachment_needs_no_request_and_a_refusal_says_desktop_too_old(
    cx: &mut TestAppContext,
) {
    let cache = scratch("attachment");
    let (state, key, rx) = phone_state(cx);
    std::fs::write(
        cache.join(attachment_cache_name("s1", "zcode-artifact://a/b")),
        b"img",
    )
    .unwrap();
    let (weak, k, c) = (state.downgrade(), key.clone(), Some(cache.clone()));
    let task = cx.spawn(async move |mut cx| {
        let source = RemoteImage::Attachment {
            session: "s1".into(),
            reference: "zcode-artifact://a/b".into(),
        };
        fetch(weak, k, source, c, &mut cx).await
    });
    cx.run_until_parked();
    assert!(rx.try_recv().is_err(), "served from the disk cache");
    assert_eq!(cx.foreground_executor().block_test(task).unwrap(), b"img");

    let (weak, k) = (state.downgrade(), key.clone());
    let task = cx.spawn(async move |mut cx| {
        let source = RemoteImage::DesktopFile(PathBuf::from("/x/y.png"));
        fetch(weak, k, source, None, &mut cx).await
    });
    cx.run_until_parked();
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], FILE_READ_METHOD);
    let refusal = json!({"code": -32601, "message": "not allowed",
        "data": {PHONE_SCOPE_REFUSAL: true}});
    state.update(cx, |s, cx| {
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            None,
            Some(refusal),
            cx,
        )
    });
    assert_eq!(
        cx.foreground_executor().block_test(task).unwrap_err(),
        crate::backend::raw_query::DESKTOP_TOO_OLD
    );
}
