use super::*;
use crate::app::test_support::RUNTIME_TEST_LOCK;
use gpui::{TestApp, TestAppWindow, px, size};
use std::sync::Arc;

const PRIMARY: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

#[test]
fn typing_merges_and_other_edits_split() {
    let r = |a: usize, b: usize| a..b;
    assert_eq!(classify(&r(3, 3), "x"), Some(Run::Typing(4)));
    assert_eq!(classify(&r(3, 3), " "), None, "a space ends the word");
    assert_eq!(classify(&r(3, 3), "pasted"), None);
    assert_eq!(classify(&r(2, 3), ""), Some(Run::Deleting(2)));
    assert!(continues(
        Some(Run::Typing(4)),
        &r(4, 4),
        Some(Run::Typing(5))
    ));
    assert!(
        !continues(Some(Run::Typing(4)), &r(9, 9), Some(Run::Typing(10))),
        "typing elsewhere after a caret jump is a new step"
    );
    assert!(continues(
        Some(Run::Deleting(5)),
        &r(4, 5),
        Some(Run::Deleting(4))
    ));
    assert!(!continues(
        Some(Run::Typing(5)),
        &r(4, 5),
        Some(Run::Deleting(4))
    ));
}

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
fn undo_and_redo_step_through_words_and_deletes() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);

    for c in "hello world".chars() {
        window.simulate_input(&c.to_string());
    }
    window.simulate_keystroke("backspace");
    window.simulate_keystroke("backspace");
    assert_eq!(text(&window), "hello wor");

    window.simulate_keystroke(&format!("{PRIMARY}-z"));
    assert_eq!(text(&window), "hello world", "both deletes undo together");
    window.simulate_keystroke(&format!("{PRIMARY}-z"));
    assert_eq!(text(&window), "hello ", "the second word is one step");
    window.simulate_keystroke(&format!("{PRIMARY}-z"));
    assert_eq!(text(&window), "hello");
    window.simulate_keystroke(&format!("{PRIMARY}-z"));
    assert_eq!(text(&window), "");

    window.simulate_keystroke(&format!("{PRIMARY}-shift-z"));
    assert_eq!(text(&window), "hello");
    if !cfg!(target_os = "macos") {
        window.simulate_keystroke("ctrl-y");
        assert_eq!(text(&window), "hello ");
    }

    window.simulate_input("X");
    window.simulate_keystroke(&format!("{PRIMARY}-shift-z"));
    assert!(
        text(&window).ends_with('X'),
        "a new edit drops the redo steps"
    );

    let sent = window.update(|c, _, _| c.take_text());
    assert!(sent.ends_with('X'));
    window.simulate_keystroke(&format!("{PRIMARY}-z"));
    assert_eq!(text(&window), "", "sending clears the history");
}
