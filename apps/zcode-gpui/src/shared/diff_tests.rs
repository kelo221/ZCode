//! Golden tests for `shared/diff_view.rs` (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn detects_hunk_headers() {
    assert!(looks_like_diff("@@ -1,3 +1,4 @@\n-old\n+new\n ctx"));
}

#[test]
fn detects_git_headers() {
    assert!(looks_like_diff(
        "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@"
    ));
}

#[test]
fn plain_prose_is_not_a_diff() {
    assert!(!looks_like_diff(
        "+1 looks good to me\nthanks for the +1 review"
    ));
    assert!(!looks_like_diff("regular chat text\nnothing special"));
}

#[test]
fn yaml_front_matter_and_rules_are_not_diffs() {
    assert!(!looks_like_diff("---\ntitle: Notes\n---\nbody text"));
    assert!(!looks_like_diff("Intro\n\n---\n\nNext section"));
    // A bare ---/+++ header pair is still recognised.
    assert!(looks_like_diff("--- a/x.rs\n+++ b/x.rs\n-old\n+new"));
}

#[test]
fn balanced_add_del_blocks_count_as_diff() {
    let text = "old line one\nold line two\n+new one\n+new two\n-old one\n-old two\nkeep";
    assert!(looks_like_diff(text));
}

#[test]
fn markers_move_to_the_gutter_and_headers_stay_whole() {
    assert_eq!(
        split_marker("+let x = 1;", LineKind::Add),
        ("+", "let x = 1;")
    );
    assert_eq!(split_marker("-old", LineKind::Del), ("-", "old"));
    assert_eq!(split_marker(" ctx", LineKind::Context), (" ", "ctx"));
    assert_eq!(
        split_marker(r"\ No newline at end of file", LineKind::Context),
        ("", r"\ No newline at end of file")
    );
    assert_eq!(
        split_marker("@@ -1 +1 @@", LineKind::Hunk),
        ("", "@@ -1 +1 @@")
    );
    assert_eq!(split_marker("", LineKind::Context), ("", ""));
}

struct View;

impl gpui::Render for View {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .text_size(px(10.))
            .child(render_diff("@@ -1 +1 @@\n-old line\n+new line\n same"))
    }
}

#[test]
fn a_drag_over_the_diff_copies_code_without_markers() {
    use gpui::{Modifiers, MouseButton, MouseMoveEvent, TestApp, point, size};
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        std::sync::Arc::new(gpui::NoopTextSystem::new()),
        std::sync::Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    let mut window = app.open_window(|_, _| View);
    window.simulate_resize(size(px(400.), px(300.)));
    window.draw();
    // Start inside the code column of the first line, drag past the end.
    window.simulate_mouse_down(point(px(30.), px(18.)), MouseButton::Left);
    window.draw();
    window.simulate_event(MouseMoveEvent {
        position: point(px(390.), px(290.)),
        modifiers: Modifiers::default(),
        pressed_button: Some(MouseButton::Left),
    });
    window.draw();
    window.simulate_mouse_up(point(px(390.), px(290.)), MouseButton::Left);
    window.draw();
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    window.simulate_keystroke(&format!("{primary}-c"));
    let copied = app
        .read_from_clipboard()
        .and_then(|c| c.text())
        .unwrap_or_default();
    assert!(
        copied.ends_with("old line\nnew line\nsame"),
        "code without +/- markers, got {copied:?}"
    );
}

#[test]
fn classify_prefixes() {
    assert_eq!(classify("+added"), LineKind::Add);
    assert_eq!(classify("-removed"), LineKind::Del);
    assert_eq!(classify("@@ -1 +1 @@"), LineKind::Hunk);
    assert_eq!(classify("+++ b/file.rs"), LineKind::Meta);
    assert_eq!(classify(" context"), LineKind::Context);
    assert_eq!(classify(""), LineKind::Context);
}
