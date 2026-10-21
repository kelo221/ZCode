use super::*;
use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::ConversationState;
use gpui::{TestApp, size};
use std::sync::Arc;

#[test]
fn queue_preview_collapses_lines_and_caps_length() {
    assert_eq!(
        queue_preview("  first\n\n  second\tthird  ").as_ref(),
        "first second third"
    );
    let long = "word\n".repeat(10_000);
    let preview = queue_preview(&long);
    assert!(preview.ends_with('…'));
    assert_eq!(preview.chars().count(), QUEUE_PREVIEW_MAX_CHARS + 1);
    assert!(!preview.contains('\n'));
    assert_eq!(queue_preview("").as_ref(), "");
}

fn item(idx: usize, text: String) -> QueueItem {
    QueueItem {
        queue_item_id: format!("q{idx}"),
        text,
        state: "queued".into(),
        kind: "sendText".into(),
        attachments: vec![],
        config: Value::Null,
    }
}

#[test]
fn queue_panel_stays_bounded_with_huge_and_many_items() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key);
        s.active = Some("s1".into());
        s.draft = false;
        let mut items = vec![item(0, "line\n".repeat(3_000))];
        items.extend((1..40).map(|i| item(i, format!("follow-up {i}"))));
        s.conversations.insert(
            "s1".into(),
            ConversationState {
                subscribed: true,
                queue: Some(QueueState {
                    items,
                    auto_drain: false,
                    pause_reason: None,
                }),
                ..Default::default()
            },
        );
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    let list = app.update(|cx| cx.global::<TestTargets>().0["queue-items"]);
    assert!(list.size.height > px(0.));
    assert!(list.size.height <= px(QUEUE_LIST_MAX_HEIGHT));
}
