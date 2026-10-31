//! Pointer test for the composer auto-compact toggle.

use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::ConversationState;
use crate::shared::auto_compact_settings::{AutoCompactEntry, entry};
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

#[test]
fn the_toggle_flips_the_selected_models_entry() {
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
            std::env::temp_dir()
                .join(uuid::Uuid::now_v7().to_string())
                .join("setting.json"),
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.active = Some("s".into());
        s.draft = false;
        let mut conv = ConversationState {
            subscribed: true,
            ..Default::default()
        };
        conv.config.provider = "p".into();
        conv.config.model = "m".into();
        s.conversations.insert("s".into(), conv);
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.draw();
    let toggle = app.read(|cx| cx.global::<TestTargets>().0["composer-auto-compact"].center());
    window.simulate_click(toggle, MouseButton::Left);
    app.read(|cx| {
        assert_eq!(
            entry("p/m", cx),
            AutoCompactEntry {
                enabled: true,
                minutes: 50
            }
        );
        assert!(!entry("p/other", cx).enabled, "other models untouched");
    });
    drop(window);
    drop(state);
}
