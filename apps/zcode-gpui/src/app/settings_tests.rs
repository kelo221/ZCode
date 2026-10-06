use crate::app::root::RootView;
use crate::app::store::AppState;
use crate::app::test_support::RUNTIME_TEST_LOCK;
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::settings::{AppSettings, load_settings_from_path};
use gpui::TestApp;
use std::sync::Arc;

fn test_app(path: std::path::PathBuf, blocked: bool) -> TestApp {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(crate::app::test_support::TestTargets::default());
        Preferences::install_at(AppSettings::default(), path, Arc::new(move || blocked), cx);
    });
    app
}

#[test]
fn settings_keyboard_quickpick_back_preserve_workspace_and_draft() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-nav-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.active = Some("session".into());
        s.composer.update(cx, |c, _| c.set_text("keep this draft"));
    });
    let mut window = app.open_window(|window, cx| {
        let view = RootView::new(state.clone(), cx);
        let focus = state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        view
    });
    window.simulate_keystroke(if cfg!(target_os = "macos") {
        "cmd-,"
    } else {
        "ctrl-,"
    });
    window.read(|view, cx| {
        assert!(view.settings.open);
        assert_eq!(view.state.read(cx).active.as_deref(), Some("session"));
        assert_eq!(
            view.state.read(cx).composer.read(cx).text(),
            "keep this draft"
        );
    });
    window.simulate_keystroke("escape");
    window.read(|view, _| assert!(!view.settings.open));
    window.simulate_keystroke(if cfg!(target_os = "macos") {
        "cmd-k"
    } else {
        "ctrl-k"
    });
    window.simulate_input("settings");
    window.simulate_keystroke("enter");
    window.read(|view, _| assert!(view.settings.open));
    window.update(|view, window, cx| {
        view.close_settings(window, cx);
        assert!(
            view.state
                .read(cx)
                .composer
                .read(cx)
                .focus
                .is_focused(window)
        );
    });
}

#[test]
fn command_center_action_keeps_input_out_of_composer() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-command-focus-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|window, cx| {
        let view = RootView::new(state.clone(), cx);
        let focus = state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        view
    });
    window.update(|_, window, cx| {
        window.dispatch_action(Box::new(crate::app::quickpick::ToggleQuickPick), cx);
    });
    window.simulate_input("settings");
    window.simulate_keystroke("enter");
    window.read(|view, cx| {
        assert!(view.settings.open);
        assert!(view.state.read(cx).composer.read(cx).text().is_empty());
    });
}

#[test]
fn settings_pointer_entries_sections_and_back() {
    use crate::app::settings::SettingsSection;
    use gpui::{MouseButton, point, px, size};
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-pointer-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, cx| {
        s.composer.update(cx, |c, _| c.set_text("keep this draft"));
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.draw();
    let gear = app.read(|cx| {
        cx.global::<crate::app::test_support::TestTargets>().0["open-settings"].center()
    });
    window.simulate_click(gear, MouseButton::Left);
    window.read(|view, _| assert!(view.settings.open));
    window.draw();
    let appearance = app.read(|cx| {
        cx.global::<crate::app::test_support::TestTargets>().0["settings-appearance"].center()
    });
    window.simulate_click(appearance, MouseButton::Left);
    window.read(|view, _| assert!(view.settings.section == SettingsSection::Appearance));
    window.draw();
    let shortcuts = app.read(|cx| {
        cx.global::<crate::app::test_support::TestTargets>().0["settings-shortcuts"].center()
    });
    window.simulate_click(shortcuts, MouseButton::Left);
    window.read(|view, _| assert!(view.settings.section == SettingsSection::Shortcuts));
    window.draw();
    window.simulate_click(point(px(340.), px(32.)), MouseButton::Left);
    window.read(|view, cx| {
        assert!(!view.settings.open);
        assert_eq!(
            view.state.read(cx).composer.read(cx).text(),
            "keep this draft"
        );
    });
}

#[test]
fn shortcut_recorder_consumes_application_keys_and_restores_focus() {
    use crate::app::settings::SettingsSection;
    use gpui::MouseButton;
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-recorder-{}", uuid::Uuid::now_v7()));
    let mut app = test_app(dir.join("setting.json"), false);
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.update(|view, window, cx| {
        view.open_settings(window, cx);
        view.settings.section = SettingsSection::Shortcuts;
    });
    window.draw();
    let recorder = app.read(|cx| {
        cx.global::<crate::app::test_support::TestTargets>().0["shortcut-openCommandCenter"]
            .center()
    });
    window.simulate_click(recorder, MouseButton::Left);
    window.simulate_keystroke(if cfg!(target_os = "macos") {
        "cmd-y"
    } else {
        "ctrl-y"
    });
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert_eq!(
            owner.snapshot.shortcut_bindings.as_ref().unwrap()["openCommandCenter"],
            [if cfg!(target_os = "macos") {
                "Cmd+y"
            } else {
                "Ctrl+y"
            }]
        );
    });
    window.read(|view, _| assert!(!view.quickpick_open));
    window.update(|view, window, _| assert!(view.settings.focus.is_focused(window)));
    window.simulate_keystroke(if cfg!(target_os = "macos") {
        "cmd-y"
    } else {
        "ctrl-y"
    });
    window.read(|view, _| assert!(view.quickpick_open));
    window.simulate_keystroke("escape");
    window.draw();
    window.simulate_click(recorder, MouseButton::Left);
    window.simulate_keystroke(if cfg!(target_os = "macos") {
        "cmd-y"
    } else {
        "ctrl-y"
    });
    window.read(|view, _| assert!(!view.quickpick_open));
    window.draw();
    window.simulate_click(recorder, MouseButton::Left);
    window.simulate_keystroke("backspace");
    app.read(|cx| {
        assert!(
            cx.global::<PreferenceOwner>()
                .0
                .read(cx)
                .snapshot
                .shortcut_bindings
                .as_ref()
                .unwrap()["openCommandCenter"]
                .is_empty()
        )
    });
    window.draw();
    let reset = app.read(|cx| {
        cx.global::<crate::app::test_support::TestTargets>().0["reset-openCommandCenter"].center()
    });
    window.simulate_click(reset, MouseButton::Left);
    app.read(|cx| {
        assert!(
            !cx.global::<PreferenceOwner>()
                .0
                .read(cx)
                .snapshot
                .shortcut_bindings
                .as_ref()
                .unwrap()
                .contains_key("openCommandCenter")
        )
    });
    window.draw();
    window.simulate_click(recorder, MouseButton::Left);
    window.simulate_keystroke("escape");
    window.read(|view, _| assert!(view.settings.open));
    window.update(|view, window, _| assert!(view.settings.focus.is_focused(window)));
    window.simulate_keystroke("escape");
    window.read(|view, _| assert!(!view.settings.open));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preference_owner_serializes_and_applies_committed_values() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-pref-owner-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("setting.json");
    std::fs::write(&path, r#"{"unknown":{"kept":true}}"#).unwrap();
    let mut app = test_app(path.clone(), false);
    app.update(|cx| {
        Preferences::enqueue(PreferenceChange::FontSize(18.0), cx);
        Preferences::enqueue(PreferenceChange::RecentProject("project".into()), cx);
        Preferences::enqueue(
            PreferenceChange::Locale(crate::shared::i18n::LocalePreference::ZhCn),
            cx,
        );
        Preferences::enqueue(
            PreferenceChange::Theme(crate::shared::theme::ThemeMode::ZaiLight),
            cx,
        );
    });
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert!(!owner.saving);
        assert!(owner.error.is_none());
        assert_eq!(owner.snapshot.ui_font_size, Some(18.0));
        assert_eq!(
            owner.snapshot.recent_projects.as_deref().unwrap(),
            ["project"]
        );
        assert_eq!(crate::shared::theme::font_size_base(), 18.0);
        assert!(!cx.global::<ely_gpui_component::theme::Theme>().is_dark());
        assert_eq!(
            crate::shared::i18n::current_locale(),
            crate::shared::i18n::Locale::ZhCn
        );
        assert_eq!(crate::shared::i18n::label("Settings", "设置"), "设置");
        assert_eq!(
            crate::shared::theme_colors::color(crate::shared::theme::TEXT),
            gpui::rgb(crate::shared::theme::active_theme().text)
        );
    });
    app.update(|cx| {
        Preferences::enqueue(
            PreferenceChange::Theme(crate::shared::theme::ThemeMode::System),
            cx,
        );
    });
    app.update(|cx| {
        crate::shared::theme::apply_appearance(gpui::WindowAppearance::Dark, cx);
        assert!(cx.global::<ely_gpui_component::theme::Theme>().is_dark());
        crate::shared::theme::apply_appearance(gpui::WindowAppearance::Light, cx);
        assert!(!cx.global::<ely_gpui_component::theme::Theme>().is_dark());
        assert_eq!(
            crate::shared::theme::theme_mode(),
            crate::shared::theme::ThemeMode::System
        );
    });
    let saved = load_settings_from_path(&path);
    assert_eq!(saved.ui_font_size, Some(18.0));
    assert_eq!(saved.extra["unknown"]["kept"], true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preference_refusal_keeps_snapshot_and_runtime_unchanged() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-pref-blocked-{}", uuid::Uuid::now_v7()));
    let path = dir.join("setting.json");
    let mut app = test_app(path.clone(), true);
    app.update(|cx| Preferences::enqueue(PreferenceChange::FontSize(20.0), cx));
    app.read(|cx| {
        let owner = cx.global::<PreferenceOwner>().0.read(cx);
        assert_eq!(owner.snapshot.ui_font_size, None);
        assert_eq!(crate::shared::theme::font_size_base(), 14.0);
        assert!(owner.error.as_ref().unwrap().contains("desktop"));
        assert!(!owner.saving);
    });
    assert!(!path.exists());
}

#[path = "settings_stream_tests.rs"]
mod stream;
