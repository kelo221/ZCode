use super::*;
use crate::backend::backlog::{EventBacklog, push_event};
use crate::backend::events::attach_pump;
use crate::backend::launcher::ConnEvent;
use crate::backend::workspace::RouteSubscription;
use serde_json::json;

#[test]
fn settings_keeps_stream_and_permission_projection_alive() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-settings-stream-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let backlog = EventBacklog::new(1024 * 1024);
    app.update_entity(&state, |s, cx| {
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.active = Some("session".into());
        s.workspaces[0].subscriptions.insert(
            "conversation/session".into(),
            RouteSubscription {
                id: "sub".into(),
                log_epoch: "epoch".into(),
            },
        );
        s.composer.update(cx, |c, _| c.set_text("keep draft"));
        attach_pump(
            cx,
            s.workspaces[0].key.clone(),
            s.workspaces[0].generation,
            backlog.clone(),
            EventBacklog::new(1024 * 1024),
            rx,
        );
    });
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings(window, cx);
        view
    });
    let line = json!({ "method": "v4/conversation/frame", "params": {
        "wireVersion": 3, "kind": "complete", "deliveryKind": "initial",
        "logicalFrameId": "frame-1", "logicalFrameOrdinal": 1,
        "topic": "conversation/session", "subscriptionId": "sub", "frame": {
            "topic": "conversation/session", "subscriptionId": "sub", "fromSeq": 0, "toSeq": 1,
            "payload": { "kind": "snapshot", "snapshot": {
                "logEpoch": "epoch", "seq": 1, "revision": 1,
                "control": { "phase": "running" },
                "rows": { "firstRowId": 0, "totalCount": 1, "window": [{
                    "rowId": 0, "kind": "assistantText", "text": "streamed while Settings is open",
                    "state": "streaming"
                }] },
                "pendingInteractions": [{ "interactionId": "permission", "kind": "permission",
                    "payload": { "kind": "permission", "summary": "Allow test command?", "options": [
                        { "optionId": "allow", "kind": "allowOnce", "label": "Allow once" }
                    ] }
                }]
            } }
        }
    } }).to_string();
    assert!(push_event(
        &backlog,
        &tx,
        ConnEvent::Line(line.clone()),
        line.len()
    ));
    app.run_until_parked();
    window.draw();
    window.read(|view, cx| {
        assert!(view.settings.open);
        let state = view.state.read(cx);
        let conversation = state.active_conversation().unwrap();
        assert_eq!(conversation.rows.len(), 1);
        assert_eq!(conversation.pending_interactions.len(), 1);
        assert_eq!(conversation.phase, "running");
        assert_eq!(state.composer.read(cx).text(), "keep draft");
    });
    window.simulate_keystroke("escape");
    window.draw();
    window.read(|view, cx| {
        assert!(!view.settings.open);
        assert_eq!(
            view.state
                .read(cx)
                .active_conversation()
                .unwrap()
                .pending_interactions[0]
                .interaction_id,
            "permission"
        );
    });
    drop(tx);
}

#[test]
fn committed_preferences_hydrate_a_new_application_owner() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-pref-restart-{}", uuid::Uuid::now_v7()));
    let path = dir.join("setting.json");
    {
        let mut app = test_app(path.clone(), false);
        app.update(|cx| {
            Preferences::enqueue(PreferenceChange::FontSize(17.0), cx);
            Preferences::enqueue(
                PreferenceChange::Theme(crate::shared::theme::ThemeMode::ZaiLight),
                cx,
            );
            Preferences::enqueue(
                PreferenceChange::Shortcut("openSettings".into(), Some(vec!["Ctrl+y".into()])),
                cx,
            );
        });
    }
    let committed = load_settings_from_path(&path);
    let mut app = test_app(path.clone(), false);
    app.update(|cx| Preferences::install_at(committed, path, Arc::new(|| false), cx));
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert_eq!(owner.snapshot.ui_font_size, Some(17.0));
        assert_eq!(crate::shared::theme::font_size_base(), 17.0);
        assert!(!cx.global::<ely_gpui_component::theme::Theme>().is_dark());
        assert_eq!(
            owner.snapshot.shortcut_bindings.as_ref().unwrap()["openSettings"],
            ["Ctrl+y"]
        );
    });
    std::fs::remove_dir_all(dir).unwrap();
}
