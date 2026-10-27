use super::{SelectableText, highlight_runs, snap, word_at};
use crate::app::test_support::RUNTIME_TEST_LOCK;
use gpui::{
    Context, Font, IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, Styled, TestApp, TestAppWindow, TextRun,
    Window, div, point, px, size,
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
    assert_eq!(
        &path[word_at(path, path.len())],
        "png",
        "the boundary after a word belongs to that word"
    );
    assert_eq!(
        &path[word_at(path, 23)],
        "inspector_v2",
        "right edge of a word"
    );
    assert_eq!(&"a  b"[word_at("a  b", 2)], " ", "space after a space");
    assert_eq!(word_at("", 0), 0..0);
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

enum View {
    Plain(&'static str),
    Markdown(&'static str),
}

impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().text_size(px(10.));
        match self {
            View::Plain(text) => root.child(SelectableText::new("path", *text)),
            View::Markdown(text) => {
                root.child(crate::shared::markdown::render_markdown(text, 7, false))
            }
        }
    }
}

/// Noop text system: every char is 0.6em wide, so 6px at 10px text.
fn x_of(ix: usize) -> Pixels {
    px(ix as f32 * 6. + 1.)
}

const PRIMARY: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

fn open(view: View, height: f32) -> (TestApp, TestAppWindow<View>) {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(ely_gpui_component::init);
    let mut window = app.open_window(|_, _| view);
    window.simulate_resize(size(px(400.), px(height)));
    window.draw();
    (app, window)
}

fn copied(app: &mut TestApp, window: &mut TestAppWindow<View>) -> Option<String> {
    window.simulate_keystroke(&format!("{PRIMARY}-c"));
    app.read_from_clipboard().and_then(|c| c.text())
}

fn down(window: &mut TestAppWindow<View>, at: Point<Pixels>, click_count: usize) {
    window.simulate_event(MouseDownEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count,
        first_mouse: false,
    });
    window.draw();
}

fn up(window: &mut TestAppWindow<View>, at: Point<Pixels>, click_count: usize) {
    window.simulate_event(MouseUpEvent {
        position: at,
        button: MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count,
    });
    window.draw();
}

fn drag_to(window: &mut TestAppWindow<View>, at: Point<Pixels>) {
    window.simulate_event(MouseMoveEvent {
        position: at,
        modifiers: Modifiers::default(),
        pressed_button: Some(MouseButton::Left),
    });
    window.draw();
}

#[test]
fn drag_past_the_end_selects_and_copies() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, mut window) = open(View::Plain(r"C:\Users\V\shot.png"), 100.);

    window.simulate_mouse_down(point(x_of(3), px(5.)), MouseButton::Left);
    window.draw();
    // Released far to the right of the text: the drag must still reach the end.
    drag_to(&mut window, point(px(390.), px(5.)));
    window.simulate_mouse_up(point(px(390.), px(5.)), MouseButton::Left);
    window.draw();
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some(r"Users\V\shot.png")
    );

    down(&mut window, point(x_of(4), px(5.)), 2);
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("Users"),
        "double-click selects one word"
    );
    up(&mut window, point(x_of(4), px(5.)), 2);

    // A click below the text but outside the element clears the selection,
    // so a later copy leaves the clipboard alone.
    app.write_to_clipboard(gpui::ClipboardItem::new_string("kept".into()));
    window.simulate_click(point(px(200.), px(90.)), MouseButton::Left);
    window.draw();
    assert_eq!(copied(&mut app, &mut window).as_deref(), Some("kept"));
}

#[test]
fn double_click_selects_a_word_and_drags_by_words() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, mut window) = open(View::Plain("alpha beta gamma delta"), 100.);
    let at = point(x_of(7), px(5.));

    // The real sequence: a first click, release, then the second press.
    down(&mut window, at, 1);
    up(&mut window, at, 1);
    down(&mut window, at, 2);
    assert_eq!(copied(&mut app, &mut window).as_deref(), Some("beta"));

    drag_to(&mut window, point(x_of(13), px(5.)));
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("beta gamma"),
        "dragging forward extends to whole words"
    );
    drag_to(&mut window, point(x_of(1), px(5.)));
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("alpha beta"),
        "dragging backward keeps the double-clicked word"
    );
    up(&mut window, point(x_of(1), px(5.)), 2);

    // Right half of "beta"'s last letter (x 54..60) is the boundary at 10.
    let edge = point(px(58.), px(5.));
    down(&mut window, edge, 1);
    up(&mut window, edge, 1);
    down(&mut window, edge, 2);
    up(&mut window, edge, 2);
    assert_eq!(copied(&mut app, &mut window).as_deref(), Some("beta"));

    down(&mut window, edge, 3);
    up(&mut window, edge, 3);
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("alpha beta gamma delta"),
        "triple-click selects the block"
    );
}

const DOC: &str = "Intro text

- two
- three

| a | b |
|---|---|
| c | d |

```
code
```
";

#[test]
fn drag_runs_across_markdown_blocks_in_document_order() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, mut window) = open(View::Markdown(DOC), 600.);

    let start = point(px(1.), px(2.));
    down(&mut window, start, 1);
    drag_to(&mut window, point(px(390.), px(590.)));
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("Intro text\ntwo\nthree\na\nb\nc\nd\ncode"),
        "the drag leaves the first block and reaches the last one"
    );

    // Back up into the first line: the selection shrinks to it again.
    drag_to(&mut window, point(px(42.), px(2.)));
    up(&mut window, point(px(42.), px(2.)), 1);
    let partial = copied(&mut app, &mut window).unwrap_or_default();
    assert!(
        !partial.is_empty() && "Intro text".starts_with(&partial),
        "selection returned into the first block, got {partial:?}"
    );

    window.simulate_keystroke(&format!("{PRIMARY}-a"));
    assert_eq!(
        copied(&mut app, &mut window).as_deref(),
        Some("Intro text\ntwo\nthree\na\nb\nc\nd\ncode"),
        "select-all covers the whole document"
    );
}
