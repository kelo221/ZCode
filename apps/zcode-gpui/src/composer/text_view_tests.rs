use super::*;
use crate::app::test_support::RUNTIME_TEST_LOCK;
use gpui::{TestApp, TestAppWindow};
use std::sync::Arc;

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
    window.simulate_resize(size(px(600.), px(900.)));
    window.update(|c, window, cx| window.focus(&c.focus, cx));
    window.draw();
    window
}

fn press(window: &mut TestAppWindow<Composer>, ix: f32, click_count: usize) {
    window.simulate_event(gpui::MouseDownEvent {
        position: char_x(ix),
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count,
        first_mouse: false,
    });
    window.draw();
}

fn release(window: &mut TestAppWindow<Composer>, ix: f32, click_count: usize) {
    window.simulate_event(gpui::MouseUpEvent {
        position: char_x(ix),
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count,
    });
    window.draw();
}

/// Noop text system: 0.6em per char at 14px, after the input's 4px padding.
fn char_x(ix: f32) -> Point<Pixels> {
    Point::new(px(4. + ix * 0.6 * ui_size(14.)), px(8.))
}

fn selected(window: &TestAppWindow<Composer>) -> String {
    window.read(|c, _| {
        c.selection()
            .map(|r| c.content[r].to_string())
            .unwrap_or_default()
    })
}

#[test]
fn double_click_selects_a_word_and_drag_extends_by_words() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);
    window.update(|c, _, cx| {
        c.set_text("alpha beta gamma\nsecond line");
        cx.notify();
    });
    window.draw();

    press(&mut window, 7.5, 1);
    release(&mut window, 7.5, 1);
    press(&mut window, 7.5, 2);
    assert_eq!(selected(&window), "beta");
    window.simulate_event(gpui::MouseMoveEvent {
        position: char_x(13.5),
        modifiers: gpui::Modifiers::default(),
        pressed_button: Some(MouseButton::Left),
    });
    window.draw();
    assert_eq!(selected(&window), "beta gamma", "drag extends by words");
    release(&mut window, 13.5, 2);

    press(&mut window, 2.5, 3);
    release(&mut window, 2.5, 3);
    assert_eq!(
        selected(&window),
        "alpha beta gamma",
        "triple-click selects the line, not the whole message"
    );
}

#[test]
fn paragraph_is_the_line_around_the_index() {
    assert_eq!(paragraph_at("ab\ncd\nef", 4), 3..5);
    assert_eq!(paragraph_at("ab\ncd", 0), 0..2);
    assert_eq!(paragraph_at("ab\n", 3), 3..3);
}

#[test]
fn reveal_offset_scrolls_only_when_the_caret_leaves_the_viewport() {
    let viewport = Bounds::new(Point::new(px(0.), px(100.)), size(px(200.), px(160.)));
    let caret = |y: f32| Bounds::new(Point::new(px(4.), px(y)), size(px(1.5), px(20.)));
    let origin = Point::new(px(0.), px(0.));
    assert_eq!(reveal_offset(caret(120.), viewport, origin), None);
    assert_eq!(
        reveal_offset(caret(300.), viewport, origin),
        Some(Point::new(px(0.), px(-60.)))
    );
    let scrolled = Point::new(px(0.), px(-200.));
    assert_eq!(
        reveal_offset(caret(80.), viewport, scrolled),
        Some(Point::new(px(0.), px(-180.)))
    );
    // Never scrolls past the top of the content.
    assert_eq!(
        reveal_offset(caret(0.), viewport, Point::new(px(0.), px(-40.))),
        Some(Point::new(px(0.), px(0.)))
    );
}

#[test]
fn long_text_caps_the_input_height_and_keeps_the_caret_visible() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    let mut window = composer_window(&mut app);
    window.update(|c, _, cx| {
        c.set_text(&"line\n".repeat(200));
        cx.notify();
    });
    window.draw();
    let handle: gpui::AnyWindowHandle = window.handle().into();
    app.update(|cx| {
        cx.update_window(handle, |_, window, cx| window.simulate_next_frame(cx))
            .unwrap()
    });
    window.draw();
    let (viewport, offset, caret) = window.read(|c, _| {
        (
            c.view.scroll.bounds(),
            c.view.scroll.offset(),
            c.view.caret_bounds.get().expect("caret painted"),
        )
    });
    assert!(viewport.size.height <= px(ui_size(COMPOSER_MAX_HEIGHT)));
    assert!(offset.y < px(0.), "the input scrolled to the caret");
    assert!(caret.bottom() <= viewport.bottom() + px(0.5));
    assert!(caret.top() >= viewport.top() - px(0.5));
}
