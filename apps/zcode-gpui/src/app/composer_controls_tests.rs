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
fn pointer_queue_restore_goal_and_ended_child_are_reachable() {
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
    let key = app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.draft = false;
        let mut conv = ConversationState::default();
        conv.apply_snapshot(&json!({"revision":7,"goal":{"objective":"Finish","status":"paused","iteration":1},"availability":{"queueEdit":{"allowed":true},"resumeGoal":{"allowed":true}},"queue":{"items":[{"queueItemId":"item","text":"restored","kind":"sendText","dispatch":{"state":"queued"}}]},"subagents":{"revision":4,"childSessionIds":[],"running":[],"endedTotal":1}}));
        s.conversations.insert("parent".into(), conv);
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.draw();
    let edit = app.update(|cx| cx.global::<TestTargets>().0["q-edit-item"]);
    window.simulate_click(edit.center(), MouseButton::Left);
    let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(command["params"]["type"], "deleteQueueItem");
    app.update_entity(&state, |s, cx| {
        assert!(s.composer.read(cx).text().is_empty());
        s.handle_response(
            &key,
            command["id"].as_u64().unwrap(),
            Some(json!({"status":"accepted"})),
            None,
            cx,
        );
        assert_eq!(s.composer.read(cx).text(), "restored");
    });
    window.update(|view, _, cx| {
        view.dock_open = true;
        view.agents_expanded = true;
        cx.notify();
    });
    window.draw();
    let mut directory_request = None;
    while let Ok(line) = rx.try_recv() {
        let request: Value = serde_json::from_str(&line).unwrap();
        if request["method"] == "session/subagents" {
            directory_request = Some(request);
        }
    }
    let goal = app.update(|cx| cx.global::<TestTargets>().0["goal-resume"]);
    window.simulate_click(goal.center(), MouseButton::Left);
    let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(command["params"]["type"], "resumeGoal");
    assert_eq!(command["params"]["baseRevision"], 7);
    let directory_request = directory_request.expect("ended directory query");
    app.update_entity(&state, |s, cx| s.handle_response(&key, directory_request["id"].as_u64().unwrap(), Some(json!({"revision":4,"childSessionIds":["ended"],"running":[],"ended":{"total":1,"items":[{"childSessionId":"ended","title":"Research","subagentType":"Explore","status":"success"}]}})), None, cx));
    window.draw();
    let open = app.update(|cx| cx.global::<TestTargets>().0["ended-open-ended"]);
    window.simulate_click(open.center(), MouseButton::Left);
    window.simulate_input("not parent input");
    window.simulate_keystroke("enter");
    window.read(|view, cx| {
        let state = view.state.read(cx);
        assert_eq!(state.viewing_child.as_deref(), Some("ended"));
        assert_eq!(state.composer.read(cx).text(), "restored");
    });
    window.update(|view, window, cx| {
        view.close_child_conversation(window, cx);
        view.open_settings(window, cx);
    });
    for (id, section) in [
        (
            "settings-plugins",
            crate::app::settings::SettingsSection::Plugins,
        ),
        ("settings-mcp", crate::app::settings::SettingsSection::Mcp),
        (
            "settings-usage",
            crate::app::settings::SettingsSection::Usage,
        ),
    ] {
        window.draw();
        let target = app.update(|cx| cx.global::<TestTargets>().0[id]);
        window.simulate_click(target.center(), MouseButton::Left);
        window.draw();
        window.read(|view, cx| {
            assert!(view.settings.section == section);
            assert_eq!(view.state.read(cx).composer.read(cx).text(), "restored");
        });
    }
}
