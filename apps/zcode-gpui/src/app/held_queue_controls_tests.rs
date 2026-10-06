use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::ConversationState;
use gpui::{MouseButton, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn held_queue_clear_keep_and_cancel_are_pointer_reachable() {
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
    let (tx, rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.workspaces[0].pumping = true;
        s.active_workspace = Some(key);
        s.active = Some("session".into());
        s.draft = false;
        let mut conv = ConversationState::default();
        conv.apply_snapshot(&json!({"revision":1,"control":{"phase":"completedSuccess","canStop":false},"inputRouting":{"mode":"choice"},"queue":{"autoDrain":false,"items":[{"queueItemId":"reviewed","text":"held"}]},"config":{"followupMode":"queue"}}));
        s.conversations.insert("session".into(), conv);
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
    while rx.try_recv().is_ok() {}
    for (target, expected) in [
        ("held-queue-cancel", None),
        ("held-queue-clear", Some("clearQueueAndSend")),
        ("held-queue-keep", Some("keepQueueAndSend")),
    ] {
        app.update_entity(&state, |s, cx| {
            s.composer.update(cx, |c, _| c.set_text("held decision"))
        });
        window.draw();
        let send = app.read(|cx| cx.global::<TestTargets>().0["send-btn"].center());
        window.simulate_click(send, MouseButton::Left);
        assert!(rx.try_recv().is_err());
        app.read_entity(&state, |s, cx| {
            assert!(s.held_confirmation.is_some());
            assert_eq!(s.composer.read(cx).text(), "held decision");
        });
        window.draw();
        let choice = app.read(|cx| cx.global::<TestTargets>().0[target].center());
        window.simulate_click(choice, MouseButton::Left);
        if let Some(expected) = expected {
            let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(
                command["params"]["payload"]["heldQueueDisposition"],
                expected
            );
            assert_eq!(
                command["params"]["payload"]["expectedHeldQueueItemIds"],
                json!(["reviewed"])
            );
            let key = app.read_entity(&state, |s, _| s.active_ws_key().unwrap());
            app.update_entity(&state, |s, cx| {
                s.handle_response(
                    &key,
                    command["id"].as_u64().unwrap(),
                    Some(json!({"status":"accepted"})),
                    None,
                    cx,
                )
            });
        } else {
            assert!(rx.try_recv().is_err());
            app.read_entity(&state, |s, cx| {
                assert_eq!(s.composer.read(cx).text(), "held decision")
            });
        }
        app.read_entity(&state, |s, _| {
            assert!(s.held_confirmation.is_none());
            assert_eq!(
                s.conversations["session"]
                    .queue
                    .as_ref()
                    .unwrap()
                    .items
                    .len(),
                1
            );
        });
    }
}
