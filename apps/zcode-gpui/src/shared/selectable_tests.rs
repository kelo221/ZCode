use super::{SelectableText, highlight_runs, snap, word_at};
use crate::app::test_support::RUNTIME_TEST_LOCK;
use gpui::{
    Context, Font, IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement, Render, Styled, TestApp, TextRun, Window, div, point, px, size,
};
use std::sync::Arc;

fn run(len: usize) -> TextRun {
    TextRun {
        len,
        font: Font::default(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[test]
fn words_and_char_boundaries() {
    let path = r"C:\Users\V\inspector_v2.png";
    assert_eq!(&path[word_at(path, 12)], "inspector_v2");
    assert_eq!(&path[word_at(path, 2)], r"\");
    assert_eq!(word_at(path, path.len()), path.len()..path.len());
    assert_eq!(snap("aé", 2), 1, "inside a two-byte char snaps back");
}

#[test]
fn selection_splits_runs_at_its_edges() {
    let bg = gpui::red();
    let runs = highlight_runs(vec![run(4), run(4)], 2..6, bg);
    let lens: Vec<_> = runs.iter().map(|r| r.len).collect();
    let marked: Vec<_> = runs.iter().map(|r| r.background_color.is_some()).collect();
    assert_eq!(lens, [2, 2, 2, 2]);
    assert_eq!(marked, [false, true, true, false]);
    assert_eq!(highlight_runs(vec![run(3)], 1..1, bg).len(), 1);
}

struct View;

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .text_size(px(10.))
            .child(SelectableText::new("path", r"C:\Users\V\shot.png"))
    }
}

/// Noop text system: every char is 0.6em wide, so 6px at 10px text.
fn x_of(ix: usize) -> gpui::Pixels {
    px(ix as f32 * 6. + 1.)
}

#[test]
fn drag_past_the_end_selects_and_copies() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    let mut window = app.open_window(|_, _| View);
    window.simulate_resize(size(px(400.), px(100.)));
    window.draw();
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };

    window.simulate_mouse_down(point(x_of(3), px(5.)), MouseButton::Left);
    window.draw();
    // Released far to the right of the text: the drag must still reach the end.
    window.simulate_event(MouseMoveEvent {
        position: point(px(390.), px(5.)),
        modifiers: Modifiers::default(),
        pressed_button: Some(MouseButton::Left),
    });
    window.simulate_mouse_up(point(px(390.), px(5.)), MouseButton::Left);
    window.draw();
    window.simulate_keystroke(&format!("{primary}-c"));
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some(r"Users\V\shot.png")
    );

    window.simulate_event(MouseDownEvent {
        position: point(x_of(4), px(5.)),
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    window.draw();
    window.simulate_keystroke(&format!("{primary}-c"));
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("Users"),
        "double-click selects one word"
    );

    // A click below the text but outside the element clears the selection,
    // so a later copy leaves the clipboard alone.
    app.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    window.simulate_click(point(px(200.), px(90.)), MouseButton::Left);
    window.draw();
    window.simulate_keystroke(&format!("{primary}-c"));
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("kept")
    );
}
