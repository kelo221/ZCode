use super::{MobilePage, is_compact};
use crate::app::quickpick::{QuickPickAction, QuickPickItemAction, execute_quickpick_action};
use crate::app::{root::RootView, store::AppState, test_support::*};
use crate::shared::preferences::Preferences;
use crate::shared::settings::AppSettings;
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

fn app() -> TestApp {
    let path = std::env::temp_dir()
        .join(format!("gpui-mobile-shell-{}", uuid::Uuid::now_v7()))
        .join("setting.json");
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(AppSettings::default(), path, Arc::new(|| false), cx);
    });
    app
}

fn targets(app: &mut TestApp) -> Vec<String> {
    app.read(|cx| cx.global::<TestTargets>().0.keys().cloned().collect())
}

fn resize(window: &mut gpui::TestAppWindow<RootView>, width: f32) {
    let requested = size(px(width), px(800.));
    window.simulate_resize(requested);
    // 测试平台的 resize 回调不会同步 viewport（见 activity_tests）；显式同步后再验证布局。
    window.update(|_, window, cx| window.bounds_changed(cx));
}

fn clear_targets(app: &mut TestApp) {
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
}

#[test]
fn phone_layout_starts_at_767_px() {
    assert!(is_compact(411.));
    assert!(is_compact(767.));
    assert!(!is_compact(768.));
}

#[test]
fn phone_pages_swap_sidebar_and_chat_and_back_returns_home() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    resize(&mut window, 411.);
    window.draw();
    let home = targets(&mut app);
    assert!(home.contains(&"workspace-sidebar".to_string()));
    assert!(!home.contains(&"mobile-back-home".to_string()));

    // An explicit "New task" opens the chat page without the sidebar.
    window.update(|view, window, cx| {
        execute_quickpick_action(
            view,
            &QuickPickItemAction::Command(QuickPickAction::NewTask),
            window,
            cx,
        );
    });
    clear_targets(&mut app);
    window.draw();
    let chat = targets(&mut app);
    assert!(chat.contains(&"mobile-back-home".to_string()));
    assert!(!chat.contains(&"workspace-sidebar".to_string()));

    let back = app.read(|cx| cx.global::<TestTargets>().0["mobile-back-home"].center());
    window.simulate_click(back, MouseButton::Left);
    window.read(|view, _| assert_eq!(view.mobile_page, MobilePage::Home));
}

#[test]
fn back_closes_the_topmost_layer_first() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    resize(&mut window, 411.);
    window.update(|view, window, cx| {
        view.show_chat_page();
        view.open_settings(window, cx);
        view.open_command_center(window, cx);

        assert!(view.handle_back(window, cx));
        assert!(!view.quickpick_open);
        assert!(view.settings.open);

        assert!(view.handle_back(window, cx));
        assert!(!view.settings.open);
        assert_eq!(view.mobile_page, MobilePage::Chat);

        assert!(view.handle_back(window, cx));
        assert_eq!(view.mobile_page, MobilePage::Home);
        assert!(!view.handle_back(window, cx));
    });
}

#[test]
fn wide_windows_keep_the_desktop_layout_on_either_page() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    resize(&mut window, 1280.);
    window.update(|view, _, _| view.show_chat_page());
    window.draw();
    let wide = targets(&mut app);
    assert!(wide.contains(&"workspace-sidebar".to_string()));
    assert!(!wide.contains(&"mobile-back-home".to_string()));
}

#[test]
fn phone_build_hides_the_desktop_dock_and_terminal() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.update(|view, _, cx| assert!(view.desktop_tools(cx)));
    app.update(|cx| state.update(cx, |s, _| s.phone_client.enabled = true));
    window.update(|view, _, cx| {
        assert!(!view.desktop_tools(cx));
        // An auto-open from a plan or a stale flag must not render the dock.
        view.dock_open = true;
        view.term_open = true;
    });
    resize(&mut window, 411.);
    window.draw();
}
