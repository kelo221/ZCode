use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::json;
use std::sync::Arc;

#[test]
fn restored_draft_thinking_selection_uses_the_existing_override() {
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
        s.active_workspace = Some(key.clone());
        let mut catalog = crate::composer::catalog::WorkspaceConfig::default();
        catalog.apply_state(&json!({"configOptions":[{"id":"model","currentValue":"test/model","options":[{"value":"test/model","name":"Model","modelThoughtLevels":["high"]}]}]}));
        s.workspace_configs.insert(key, catalog);
        s.enable_plan_override();
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["thought-menu"]);
    window.simulate_click(target.center(), MouseButton::Left);
    window.draw();
    window.simulate_keystroke("down");
    window.draw();
    window.simulate_keystroke("down");
    window.draw();
    window.simulate_keystroke("enter");
    window.draw();
    window.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    window.draw();
    app.update_entity(&state, |s, _| {
        assert_eq!(
            s.submission_override()["modelSelection"]["options"]["reasoningLevel"],
            "high"
        );
        assert_eq!(s.submission_override()["planEnabled"], true);
        s.draft_submission_overrides.clear();
    });
    window.draw();
    let target = app.update(|cx| cx.global::<TestTargets>().0["thought-menu"]);
    window.simulate_click(target.center(), MouseButton::Left);
    window.draw();
    window.simulate_keystroke("down");
    window.draw();
    window.simulate_keystroke("down");
    window.draw();
    window.simulate_keystroke("enter");
    window.draw();
    window.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    window.draw();
    app.update_entity(&state, |s, _| assert!(s.submission_override().is_null()));
}
