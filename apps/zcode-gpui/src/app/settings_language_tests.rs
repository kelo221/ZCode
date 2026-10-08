use crate::app::{root::RootView, store::AppState, test_support::*};
use crate::shared::i18n::{Locale, LocalePreference, current_locale, label};
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::settings::{AppSettings, load_settings_from_path};
use gpui::{MouseButton, TestApp, px, size};
use std::{path::Path, sync::Arc};

fn app(settings: AppSettings, path: &Path, desktop: bool) -> TestApp {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        cx.set_global(TestLabels::default());
        Preferences::install_at(settings, path.into(), Arc::new(move || desktop), cx);
    });
    app
}

fn assert_language(app: &TestApp, expected: Locale) {
    app.read(|cx| {
        assert_eq!(current_locale(), expected);
        assert_eq!(
            cx.global::<ely_gpui_component::i18n::I18n>().locale().tag,
            expected.as_str()
        );
        let chinese = expected == Locale::ZhCn;
        assert_eq!(
            label("Settings", "设置"),
            if chinese { "设置" } else { "Settings" }
        );
        assert_eq!(
            ely_gpui_component::i18n::text(cx, "dialog.cancel", &[]).as_ref(),
            if chinese { "取消" } else { "Cancel" }
        );
        let labels: Vec<_> = crate::app::settings::language_choices()
            .into_iter()
            .map(|choice| choice.label.to_string())
            .collect();
        assert_eq!(
            labels,
            if chinese {
                vec!["跟随系统", "英语", "简体中文"]
            } else {
                vec!["System", "English", "Chinese (Simplified)"]
            }
        );
    });
}

#[test]
fn settings_language_saved_preference_and_legacy_value_use_one_resolver() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("gpui-language-default-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("setting.json");
    for (preference, legacy, expected) in [
        (None, None, LocalePreference::EnUs),
        (Some("invalid"), Some("zh-CN"), LocalePreference::EnUs),
        (Some(""), None, LocalePreference::EnUs),
        (None, Some("invalid"), LocalePreference::EnUs),
        (None, Some("zh-CN"), LocalePreference::ZhCn),
        (Some("zh"), None, LocalePreference::ZhCn),
        (Some("en-US"), Some("zh-CN"), LocalePreference::EnUs),
        (Some("system"), Some("en-US"), LocalePreference::System),
    ] {
        let settings = AppSettings {
            locale_preference: preference.map(str::to_owned),
            locale: legacy.map(str::to_owned),
            ..Default::default()
        };
        assert_eq!(settings.language_preference(), expected);
        let before = serde_json::to_vec(&settings).unwrap();
        std::fs::write(&path, &before).unwrap();
        let app = app(settings.clone(), &path, false);
        assert_language(&app, expected.resolve());
        app.read(|cx| assert_eq!(cx.global::<PreferenceOwner>().0.read(cx).snapshot, settings));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_language_selector_commits_relocalizes_and_survives_reload() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("gpui-language-ui-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("setting.json");
    std::fs::write(&path, r#"{"userContent":"用户内容 / user content"}"#).unwrap();
    let profile = root.join("synthetic-profile.md");
    let content = "Synthetic user content / 合成用户内容\n";
    std::fs::write(&profile, content).unwrap();
    let mut app = app(load_settings_from_path(&path), &path, false);
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings(window, cx);
        view
    });
    window.simulate_resize(size(px(1280.), px(820.)));
    assert_language(&app, Locale::EnUs);
    for (key, expected) in [
        ("end", LocalePreference::ZhCn),
        ("home", LocalePreference::System),
        ("down", LocalePreference::EnUs),
    ] {
        window.draw();
        let selector = app.read(|cx| cx.global::<TestTargets>().0["settings-locale"].center());
        window.simulate_click(selector, MouseButton::Left);
        window.simulate_keystroke(key);
        window.simulate_keystroke("enter");
        let saved = load_settings_from_path(&path);
        assert_eq!(saved.locale_preference.as_deref(), Some(expected.as_str()));
        assert_eq!(saved.language_preference(), expected);
        assert_language(&app, expected.resolve());
        window.draw();
        app.read(|cx| {
            assert_eq!(
                cx.global::<TestLabels>().0["settings-search"],
                label("Search settings", "搜索设置")
            );
        });
        assert_eq!(saved.extra["userContent"], "用户内容 / user content");
        assert_eq!(std::fs::read_to_string(&profile).unwrap(), content);
        window.update(|view, window, cx| {
            view.close_settings(window, cx);
            view.open_settings(window, cx);
        });
        app.update(|cx| Preferences::install_at(saved, path.clone(), Arc::new(|| false), cx));
        assert_language(&app, expected.resolve());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_language_refusal_keeps_committed_locale_disk_and_ely_unchanged() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("gpui-language-refused-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("setting.json");
    let before = r#"{"localePreference":"en-US","locale":"en-US"}"#;
    for mode in ["read-only", "suspended", "desktop"] {
        std::fs::write(&path, before).unwrap();
        let mut app = app(load_settings_from_path(&path), &path, mode == "desktop");
        app.update(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.clone();
            owner.update(cx, |owner, _| {
                owner.read_only = mode == "read-only";
                owner.suspended = mode == "suspended";
            });
            Preferences::enqueue(PreferenceChange::Locale(LocalePreference::ZhCn), cx);
        });
        app.read(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.read(cx);
            assert!(!owner.saving);
            assert!(owner.error.is_some());
            assert_eq!(owner.snapshot.language_preference(), LocalePreference::EnUs);
        });
        assert_language(&app, Locale::EnUs);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        if mode != "desktop" {
            let state = app.new_entity(AppState::for_test);
            let mut window = app.open_window(|window, cx| {
                let mut view = RootView::new(state.clone(), cx);
                view.open_settings(window, cx);
                view
            });
            window.draw();
            let selector = app.read(|cx| cx.global::<TestTargets>().0["settings-locale"].center());
            window.simulate_click(selector, MouseButton::Left);
            window.simulate_keystroke("end");
            window.simulate_keystroke("enter");
            assert_language(&app, Locale::EnUs);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
