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
