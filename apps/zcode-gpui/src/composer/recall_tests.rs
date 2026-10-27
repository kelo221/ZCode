use super::*;
use crate::app::test_support::RUNTIME_TEST_LOCK;
use gpui::{TestApp, px, size};
use std::sync::Arc;

fn sent() -> Vec<String> {
    vec!["newest".into(), "middle".into(), "oldest".into()]
}

#[test]
fn up_walks_older_and_down_walks_back_to_an_empty_box() {
    let s = sent();
    assert_eq!(
        recall_step(&s, None, "", true),
        Some((Some(0), "newest".into()))
    );
    assert_eq!(
        recall_step(&s, Some(0), "newest", true),
        Some((Some(1), "middle".into()))
    );
    assert_eq!(
        recall_step(&s, Some(2), "oldest", true),
        None,
        "nothing older"
    );
    assert_eq!(
        recall_step(&s, Some(1), "middle", false),
        Some((Some(0), "newest".into()))
    );
    assert_eq!(
        recall_step(&s, Some(0), "newest", false),
        Some((None, String::new()))
    );
    assert_eq!(recall_step(&s, None, "", false), None);
}

#[test]
fn typed_text_is_never_replaced() {
    let s = sent();
    assert_eq!(recall_step(&s, None, "draft", true), None);
    assert_eq!(recall_step(&s, Some(0), "newest edited", true), None);
    assert_eq!(recall_step(&[], None, "", true), None);
}

#[test]
fn arrow_keys_recall_through_the_window() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let mut window = app.open_window(|_, cx| {
        let mut c = Composer::new(cx);
        c.recall.source = Some(Rc::new(|_: &App| sent()));
        c
    });
    window.simulate_resize(size(px(600.), px(200.)));
    window.update(|c, window, cx| window.focus(&c.focus, cx));
    window.draw();
    let text = |w: &gpui::TestAppWindow<Composer>| w.read(|c, _| c.text().to_string());

    window.simulate_keystroke("up");
    window.draw();
    assert_eq!(text(&window), "newest");
    window.simulate_keystroke("up");
    window.draw();
    assert_eq!(text(&window), "middle");
    window.simulate_keystroke("down");
    window.draw();
    window.simulate_keystroke("down");
    window.draw();
    assert_eq!(text(&window), "", "past the newest the box is empty again");
}
