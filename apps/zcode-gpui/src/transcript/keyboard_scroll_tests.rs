//! Transcript keyboard scrolling (PageUp/PageDown/Home/End) and the
//! "Load earlier messages" history pager, driven through a real window.

use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::ConversationState;
use crate::transcript::scroll::{TranscriptKey, page_step, transcript_key};
use gpui::{Entity, Keystroke, MouseButton, Pixels, TestApp, TestAppWindow, point, px, size};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc::Receiver};

#[test]
fn keys_map_to_transcript_scrolls() {
    let key = |s: &str| transcript_key(&Keystroke::parse(s).unwrap());
    assert_eq!(key("pageup"), Some(TranscriptKey::PageUp));
    assert_eq!(key("pagedown"), Some(TranscriptKey::PageDown));
    assert_eq!(key("home"), Some(TranscriptKey::Top));
    assert_eq!(key("ctrl-end"), Some(TranscriptKey::Bottom));
    assert_eq!(key("shift-home"), None, "Shift extends selections");
    assert_eq!(key("ctrl-pageup"), None, "Ctrl+PageUp is tab switching");
    assert_eq!(key("up"), None);
    assert_eq!(page_step(px(800.)), px(760.));
    assert_eq!(page_step(px(60.)), px(30.), "tiny viewports still advance");
}

fn rows(ids: impl Iterator<Item = u64>) -> Vec<Value> {
    ids.map(|id| json!({ "rowId": id, "kind": "userInput", "text": format!("message {id}") }))
        .collect()
}

/// A focused composer over a subscribed conversation holding rows
/// `first..=last` of a conversation that starts at row 1.
fn open(
    first: u64,
    last: u64,
) -> (
    TestApp,
    Entity<AppState>,
    TestAppWindow<RootView>,
    Receiver<String>,
) {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
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
    app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.workspaces[0].pumping = true;
        s.active_workspace = Some(key);
        s.active = Some("s".into());
        s.draft = false;
        let mut conversation = ConversationState::default();
        conversation.apply_snapshot(&json!({
            "revision": 1,
            "inputRouting": { "mode": "startNow" },
            "control": { "canStop": false },
            "rows": { "firstRowId": 1, "totalCount": last, "window": rows(first..=last) },
        }));
        conversation.subscribed = true;
        s.conversations.insert("s".into(), conversation);
    });
    let mut window = app.open_window(|window, cx| {
        let view = RootView::new(state.clone(), cx);
        let focus = state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        view
    });
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    window.draw();
    (app, state, window, rx)
}

/// The first visible row, its hidden part, and whether the tail is followed.
/// Rows, not `scroll_px_offset_for_scrollbar`: that sums measured rows only.
fn scroll(window: &mut TestAppWindow<RootView>) -> ((usize, Pixels), bool) {
    window.draw();
    window.update(|view, _, _| {
        let top = view.list_state.logical_scroll_top();
        (
            (top.item_ix, top.offset_in_item),
            view.follow_bottom.load(Ordering::Relaxed),
        )
    })
}

#[test]
fn page_keys_scroll_the_transcript_from_the_composer() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (_app, _state, mut window, _rx) = open(1, 150);
    let (tail, follow) = scroll(&mut window);
    assert!(
        follow && tail.0 > 100,
        "opens on the tail of 150 rows: {tail:?}"
    );

    window.simulate_keystroke("pageup");
    let (one, follow) = scroll(&mut window);
    assert!(one < tail, "PageUp moved up: {tail:?} -> {one:?}");
    assert!(
        tail.0 - one.0 >= 3,
        "a page is several rows: {tail:?} -> {one:?}"
    );
    assert!(!follow, "reading history stops following appends");
    window.simulate_keystroke("pageup");
    let (two, _) = scroll(&mut window);
    assert!(two < one);

    window.simulate_keystroke("pagedown");
    let (down, follow) = scroll(&mut window);
    assert_eq!(down.0, one.0, "PageDown undoes one PageUp");
    assert!(!follow);
    window.simulate_keystroke("pagedown");
    let (back, follow) = scroll(&mut window);
    assert_eq!(back, tail, "the second PageDown lands on the tail");
    assert!(follow, "and follows appends again");
    window.simulate_keystroke("pagedown");
    assert_eq!(
        scroll(&mut window),
        (tail, true),
        "PageDown at the tail stays put"
    );
}

#[test]
fn jump_to_latest_shows_while_reading_history_and_returns_to_the_tail() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, _state, mut window, _rx) = open(1, 150);
    let shown = |app: &mut TestApp| {
        app.read(|cx| cx.global::<TestTargets>().0.get("jump-to-latest").copied())
    };
    let (tail, _) = scroll(&mut window);
    assert!(shown(&mut app).is_none(), "hidden while following the tail");

    window.simulate_keystroke("pageup");
    scroll(&mut window);
    let button = shown(&mut app).expect("offered after scrolling up");
    window.simulate_click(button.center(), MouseButton::Left);
    assert_eq!(
        scroll(&mut window),
        (tail, true),
        "back on the tail, following"
    );
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    assert!(shown(&mut app).is_none(), "and the button is gone");
}

#[test]
fn files_dropped_on_the_transcript_attach_to_the_composer() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (app, state, mut window, _rx) = open(1, 20);
    let file = std::env::temp_dir().join(format!("drop-{}.txt", uuid::Uuid::now_v7()));
    std::fs::write(&file, "dropped").unwrap();
    let over_rows = point(px(760.), px(300.));
    window.simulate_event(gpui::FileDropEvent::Entered {
        position: over_rows,
        paths: gpui::ExternalPaths(vec![file.clone()].into()),
    });
    window.draw();
    window.simulate_event(gpui::FileDropEvent::Submit {
        position: over_rows,
    });
    window.draw();
    let attached = app.read_entity(&state, |s, cx| s.composer.read(cx).attachments().len());
    let _ = std::fs::remove_file(&file);
    assert_eq!(attached, 1, "a drop over the transcript attaches the file");
}

#[test]
fn home_end_stay_with_the_composer_until_the_transcript_is_clicked() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, mut window, _rx) = open(1, 150);
    app.update_entity(&state, |s, cx| {
        s.composer.update(cx, |c, _| c.set_text("draft"))
    });
    let (tail, _) = scroll(&mut window);
    window.simulate_keystroke("home");
    assert_eq!(
        scroll(&mut window),
        (tail, true),
        "Home moved the composer caret only"
    );

    window.simulate_click(point(px(760.), px(300.)), MouseButton::Left);
    window.update(|view, window, cx| {
        assert!(view.transcript_focus.contains_focused(window, cx));
    });
    window.simulate_keystroke("home");
    assert_eq!(scroll(&mut window), ((0, px(0.)), false));

    window.simulate_keystroke("end");
    assert_eq!(scroll(&mut window), (tail, true));
    app.read_entity(&state, |s, cx| {
        assert_eq!(s.composer.read(cx).text(), "draft")
    });
}

#[test]
fn load_earlier_pages_before_the_oldest_loaded_row_until_the_start() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, mut window, rx) = open(201, 260);
    while rx.try_recv().is_ok() {}
    let button = app.read(|cx| cx.global::<TestTargets>().0.get("load-earlier").copied());
    let button = button.expect("history before row 201 is offered");
    window.simulate_click(button.center(), MouseButton::Left);
    let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(request["method"], "v4/conversation/rowsRange");
    assert_eq!(request["params"]["beforeRowId"], 201);
    assert_eq!(request["params"]["limit"], 200);

    let key = app.read_entity(&state, |s, _| s.workspaces[0].key.clone());
    app.update_entity(&state, |s, cx| {
        let page = json!({ "rows": rows(1..=200), "hasMore": false });
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(page), None, cx);
    });
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    app.read_entity(&state, |s, _| {
        let c = &s.conversations["s"];
        assert_eq!(c.rows.len(), 260);
        assert!(!c.has_more_history());
    });
    let shown = app.read(|cx| cx.global::<TestTargets>().0.contains_key("load-earlier"));
    assert!(!shown, "the button hides at the start of the conversation");
}
