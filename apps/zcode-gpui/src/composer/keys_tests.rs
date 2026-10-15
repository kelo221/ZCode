//! Keystroke-level composer editing through a real (test) window.

use crate::app::test_support::RUNTIME_TEST_LOCK;
use crate::composer::input::Composer;
use gpui::{ClipboardItem, TestApp, TestAppWindow, px, size};
use std::sync::Arc;

const WORD: &str = if cfg!(target_os = "macos") {
    "alt"
} else {
    "ctrl"
};
const PRIMARY: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

fn composer_window(app: &mut TestApp) -> TestAppWindow<Composer> {
    app.update(|cx| {
        ely_gpui_component::init(cx);
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let mut window = app.open_window(|_, cx| Composer::new(cx));
    window.simulate_resize(size(px(600.), px(200.)));
    window.update(|c, window, cx| window.focus(&c.focus, cx));
    window.draw();
    window
}

fn text(window: &TestAppWindow<Composer>) -> String {
    window.read(|c, _| c.text().to_string())
}

#[test]
fn word_delete_select_all_and_selection_replace() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);

    window.simulate_input("hello brave world");
    window.simulate_keystroke(&format!("{WORD}-backspace"));
    assert_eq!(text(&window), "hello brave ");

    window.simulate_keystroke(&format!("{WORD}-left"));
    window.simulate_keystroke(&format!("{WORD}-delete"));
    assert_eq!(text(&window), "hello  ");

    window.simulate_keystroke("end");
    window.simulate_keystroke("shift-left");
    window.simulate_keystroke("shift-left");
    window.simulate_input("!");
    assert_eq!(text(&window), "hello!", "typing replaces the selection");

    window.simulate_keystroke(&format!("{PRIMARY}-a"));
    window.simulate_keystroke(&format!("{PRIMARY}-x"));
    assert_eq!(text(&window), "");
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()),
        Some("hello!".to_string())
    );

    app.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
    window.simulate_keystroke(&format!("{PRIMARY}-v"));
    window.simulate_keystroke(&format!("{PRIMARY}-a"));
    window.simulate_keystroke("backspace");
    assert_eq!(text(&window), "", "backspace deletes the whole selection");
}

#[test]
fn long_text_renders_without_panicking_and_keeps_caret_at_end() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);
    let long = "word ".repeat(200);
    window.simulate_input(&long);
    window.draw();
    window.read(|c, _| {
        assert_eq!(c.caret, long.len());
        assert!(
            c.view.caret_bounds.get().is_some(),
            "caret painted from layout"
        );
    });
}

#[test]
fn empty_composer_caret_sits_on_the_placeholder_line() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);
    let empty = window.read(|c, _| c.view.caret_bounds.get().expect("caret painted"));
    window.simulate_input("a");
    window.draw();
    let typed = window.read(|c, _| c.view.caret_bounds.get().expect("caret painted"));
    assert_eq!(
        empty.origin.y, typed.origin.y,
        "the empty caret overlays the placeholder instead of a line below it"
    );
}
