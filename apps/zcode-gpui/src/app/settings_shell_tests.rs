use crate::app::{root::RootView, settings::SettingsSection, store::AppState, test_support::*};
use crate::shared::{preferences::Preferences, settings::AppSettings};
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

#[test]
fn settings_shell_replaces_workspace_sidebar_and_back_restores_same_state() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("gpui-settings-shell-{}", uuid::Uuid::now_v7()));
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            AppSettings::default(),
            root.join("setting.json"),
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |state, cx| {
        state.active_workspace = Some(state.workspaces[0].key.clone());
        state.active = Some("session".into());
        state.composer.update(cx, |c, _| c.set_text("keep draft"));
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.update(|view, _, _| {
        view.ws_collapsed.insert("collapsed-project".into());
        view.session_limit_step.insert("project".into(), 2);
        view.dock_open = true;
        view.term_open = true;
    });
    window.simulate_resize(size(px(1280.), px(820.)));
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    app.read(|cx| {
        assert!(
            cx.global::<TestTargets>()
                .0
                .contains_key("workspace-sidebar")
        )
    });
    for width in [1280., 700.] {
        window.simulate_resize(size(px(width), px(820.)));
        window.update(|view, window, cx| {
            view.open_settings_section(SettingsSection::General, window, cx)
        });
        app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
        window.draw();
        app.read(|cx| {
            let targets = &cx.global::<TestTargets>().0;
            assert!(!targets.contains_key("workspace-sidebar"));
            assert!(!targets.contains_key("open-settings"));
            assert!(targets.contains_key("settings-back"));
            assert!(targets.contains_key("settings-general"));
        });
        let back = app.read(|cx| cx.global::<TestTargets>().0["settings-back"].center());
        window.simulate_click(back, MouseButton::Left);
        app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
        window.draw();
        app.read(|cx| {
            assert!(
                cx.global::<TestTargets>()
                    .0
                    .contains_key("workspace-sidebar")
            )
        });
        window.read(|view, cx| {
            assert!(!view.settings.open);
            assert!(view.ws_collapsed.contains("collapsed-project"));
            assert_eq!(view.session_limit_step["project"], 2);
            assert!(view.dock_open);
            assert!(view.term_open);
            let state = view.state.read(cx);
            assert_eq!(state.active.as_deref(), Some("session"));
            assert_eq!(
                state.active_workspace.as_deref(),
                Some(state.workspaces[0].key.as_str())
            );
            assert_eq!(state.composer.read(cx).text(), "keep draft");
        });
    }
    assert!(!root.exists());
}
