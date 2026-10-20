use super::*;
use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use gpui::{ClipboardItem, Entity, TestApp, TestAppWindow, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn threshold_counts_utf16_units_like_desktop() {
    assert!(!is_long_paste(&"a".repeat(15_359)));
    assert!(is_long_paste(&"a".repeat(15_360)));
    // 3 UTF-8 bytes but 1 UTF-16 unit each: bytes alone must not trigger it.
    assert!(!is_long_paste(&"中".repeat(15_359)));
    assert!(is_long_paste(&"中".repeat(15_360)));
    // Astral chars count 2 units, as in JS `length`.
    assert!(is_long_paste(&"😀".repeat(7_680)));
}

#[test]
fn lines_count_every_break_style() {
    assert_eq!(count_lines(""), 0);
    assert_eq!(count_lines("one"), 1);
    assert_eq!(count_lines("a\nb\r\nc\rd"), 4);
    assert_eq!(count_lines("trailing\n"), 2);
}

#[test]
fn file_name_uses_local_wall_clock() {
    // 2026-10-14T09:05:07Z
    let ms = 1_791_968_707_000;
    assert_eq!(file_name_at(ms, 0), "pasted-text-20261014-090507.txt");
    assert_eq!(
        file_name_at(ms, 15 * 3600),
        "pasted-text-20261015-000507.txt"
    );
}

fn open(
    app: &mut TestApp,
    session: bool,
) -> (
    Entity<AppState>,
    TestAppWindow<RootView>,
    std::sync::mpsc::Receiver<String>,
) {
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, cx| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.workspaces[0].pumping = true;
        s.active_workspace = Some(s.workspaces[0].key.clone());
        if session {
            s.active = Some("parent".into());
            s.draft = false;
        }
        s.composer
            .update(cx, |composer, _| composer.set_text("keep this draft"));
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| {
        let focus = state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.bounds_changed(cx);
    });
    window.draw();
    (state, window, rx)
}

fn long_text() -> String {
    "line of pasted text\n".repeat(1_000)
}

#[test]
fn long_paste_in_a_session_uploads_a_text_attachment_and_keeps_the_draft() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let (state, mut window, rx) = open(&mut app, true);
    let key = app.update_entity(&state, |s, _| s.workspaces[0].key.clone());
    app.write_to_clipboard(ClipboardItem::new_string(long_text()));
    window.simulate_keystroke("secondary-v");
    window.draw();
    let begin: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(begin["method"], "v4/attachment/begin");
    assert_eq!(begin["params"]["mime"], "text/plain");
    assert_eq!(begin["params"]["totalBytes"], long_text().len());
    let name = begin["params"]["fileName"].as_str().unwrap().to_string();
    assert!(
        name.starts_with("pasted-text-") && name.ends_with(".txt"),
        "{name}"
    );
    let upload = begin["params"]["uploadId"].as_str().unwrap().to_string();
    app.update_entity(&state, |s, cx| {
        assert_eq!(s.composer.read(cx).text(), "keep this draft");
        s.handle_response(
            &key,
            begin["id"].as_u64().unwrap(),
            Some(json!({"uploadId":upload,"state":"committed","nextChunkIndex":1,"ref":"zcode-artifact://parent/pasted"})),
            None,
            cx,
        );
        let composer = s.composer.read(cx);
        assert_eq!(composer.attachments[0].mime, "text/plain");
        assert_eq!(composer.attachments[0].file_name, name);
        assert_eq!(composer.pasted_text_lines(&composer.attachments[0]), Some(1_001));
        assert!(composer.temp_owned.is_empty(), "the CLI owns artifact refs");
    });
}

#[test]
fn long_paste_in_a_new_draft_writes_an_owned_text_file_and_short_text_stays_inline() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let (state, mut window, rx) = open(&mut app, false);
    app.write_to_clipboard(ClipboardItem::new_string(long_text()));
    window.simulate_keystroke("secondary-v");
    window.draw();
    assert!(rx.try_recv().is_err(), "a draft never uploads");
    let path = app.update_entity(&state, |s, cx| {
        let composer = s.composer.read(cx);
        assert_eq!(composer.text(), "keep this draft");
        let att = &composer.attachments[0];
        assert_eq!(att.mime, "text/plain");
        assert!(att.file_name.starts_with("pasted-text-"));
        let path = std::path::PathBuf::from(&att.reference);
        assert_eq!(composer.temp_owned, vec![path.clone()]);
        path
    });
    assert_eq!(std::fs::read_to_string(&path).unwrap(), long_text());
    app.update_entity(&state, |s, cx| {
        s.composer.update(cx, |c, cx| c.remove_attachment(0, cx))
    });
    window.draw();
    assert!(!path.exists(), "removing the chip deletes the owned file");

    app.write_to_clipboard(ClipboardItem::new_string("short".into()));
    window.simulate_keystroke("secondary-v");
    app.update_entity(&state, |s, cx| {
        let composer = s.composer.read(cx);
        assert_eq!(composer.text(), "keep this draftshort");
        assert!(composer.attachments.is_empty());
    });
}
