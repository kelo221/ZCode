use crate::app::settings::SettingsSection;
use crate::app::settings_shortcuts::{edit_binding, shortcut_matches};
use crate::app::{root::RootView, store::AppState, test_support::*};
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::settings::{AppSettings, load_settings_from_path};
use crate::shared::shortcuts::ShortcutCommandId as Command;
use gpui::{MouseButton, TestApp, px, size};
use std::{collections::HashMap, sync::Arc};

#[test]
fn binding_edits_preserve_siblings_append_remove_and_disable() {
    let values = vec!["Ctrl+q".into(), "Ctrl+w".into()];
    assert_eq!(
        edit_binding(&values, 0, Some("Ctrl+e".into())).unwrap(),
        ["Ctrl+e", "Ctrl+w"]
    );
    assert_eq!(
        edit_binding(&values, 1, Some("Ctrl+e".into())).unwrap(),
        ["Ctrl+q", "Ctrl+e"]
    );
    assert_eq!(
        edit_binding(&values, 2, Some("Ctrl+e".into())).unwrap(),
        ["Ctrl+q", "Ctrl+w", "Ctrl+e"]
    );
    assert_eq!(edit_binding(&values, 0, None).unwrap(), ["Ctrl+w"]);
    assert!(
        edit_binding(&["Ctrl+q".into()], 0, None)
            .unwrap()
            .is_empty()
    );
    assert!(edit_binding(&values, 3, Some("Ctrl+e".into())).is_none());
    assert!(edit_binding(&values, 2, None).is_none());
    assert!(shortcut_matches(Command::OpenSettings, " SETTINGS "));
    assert!(shortcut_matches(Command::OpenWorkspace, "打开工作区"));
    assert!(!shortcut_matches(Command::OpenSettings, "unknown"));
}

#[test]
fn vector_conflicts_remain_preference_owner_validation_and_reset_semantics() {
    let mut settings = AppSettings {
        shortcut_bindings: Some(HashMap::from([
            (
                "openSettings".into(),
                vec!["Ctrl+q".into(), "Ctrl+w".into()],
            ),
            (
                "unsupportedCommand".into(),
                vec!["unsupported binding".into()],
            ),
        ])),
        ..Default::default()
    };
    let conflict = PreferenceChange::Shortcut(
        "openSettings".into(),
        Some(vec!["Ctrl+q".into(), "Shift+CmdOrCtrl+l".into()]),
    );
    assert!(conflict.validate(&settings).is_err());
    assert_eq!(
        settings.shortcut_bindings.as_ref().unwrap()["openSettings"].len(),
        2
    );
    let disable = PreferenceChange::Shortcut("openSettings".into(), Some(vec![]));
    assert!(disable.validate(&settings).is_ok());
    disable.apply(&mut settings);
    assert!(
        crate::shared::shortcut_runtime::bindings(
            Command::OpenSettings,
            settings.shortcut_bindings.as_ref()
        )
        .is_empty()
    );
    PreferenceChange::Shortcut("openSettings".into(), None).apply(&mut settings);
    assert_eq!(
        crate::shared::shortcut_runtime::bindings(
            Command::OpenSettings,
            settings.shortcut_bindings.as_ref()
        ),
        ["CmdOrCtrl+,"]
    );
    assert!(
        settings
            .shortcut_bindings
            .as_ref()
            .unwrap()
            .contains_key("unsupportedCommand")
    );
}

fn app(path: std::path::PathBuf) -> TestApp {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            AppSettings {
                shortcut_bindings: Some(HashMap::from([(
                    "openCommandCenter".into(),
                    vec!["Ctrl+q".into(), "Ctrl+w".into()],
                )])),
                ..Default::default()
            },
            path,
            Arc::new(|| false),
            cx,
        );
    });
    app
}

#[test]
fn measured_second_binding_add_remove_disable_and_reset_preserve_the_vector() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-settings-bindings-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("setting.json");
    std::fs::write(
        &path,
        r#"{"shortcutBindings":{"openCommandCenter":["Ctrl+q","Ctrl+w"]},"unknown":true}"#,
    )
    .unwrap();
    let mut app = app(path.clone());
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|view, window, cx| {
        view.open_settings_section(SettingsSection::Shortcuts, window, cx);
        view.settings.shortcut_query = "Command center".into();
    });
    window.draw();
    let recorder =
        app.read(|cx| cx.global::<TestTargets>().0["shortcut-openCommandCenter-1"].center());
    window.simulate_click(recorder, MouseButton::Left);
    window.simulate_keystroke("ctrl-e");
    app.read(|cx| {
        assert_eq!(
            cx.global::<PreferenceOwner>()
                .0
                .read(cx)
                .snapshot
                .shortcut_bindings
                .as_ref()
                .unwrap()["openCommandCenter"],
            ["Ctrl+q", "Ctrl+e"]
        )
    });
    window.read(|view, _| assert!(!view.quickpick_open));
    for (target, keystroke, expected) in [
        (
            "add-openCommandCenter",
            Some("ctrl-r"),
            vec!["Ctrl+q", "Ctrl+e", "Ctrl+r"],
        ),
        ("remove-openCommandCenter-0", None, vec!["Ctrl+e", "Ctrl+r"]),
        ("disable-openCommandCenter", None, vec![]),
    ] {
        window.draw();
        let point = app.read(|cx| cx.global::<TestTargets>().0[target].center());
        window.simulate_click(point, MouseButton::Left);
        if let Some(key) = keystroke {
            window.draw();
            let point = app
                .read(|cx| cx.global::<TestTargets>().0["shortcut-add-openCommandCenter"].center());
            window.simulate_click(point, MouseButton::Left);
            window.simulate_keystroke(key);
        }
        app.read(|cx| {
            assert_eq!(
                cx.global::<PreferenceOwner>()
                    .0
                    .read(cx)
                    .snapshot
                    .shortcut_bindings
                    .as_ref()
                    .unwrap()["openCommandCenter"],
                expected
            )
        });
    }
    window.draw();
    let reset = app.read(|cx| cx.global::<TestTargets>().0["reset-openCommandCenter"].center());
    window.simulate_click(reset, MouseButton::Left);
    let settings = load_settings_from_path(&path);
    assert!(
        !settings
            .shortcut_bindings
            .unwrap()
            .contains_key("openCommandCenter")
    );
    assert_eq!(settings.extra["unknown"], true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn measured_read_only_and_saving_controls_cannot_record_or_mutate() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "gpui-settings-bindings-blocked-{}",
        uuid::Uuid::now_v7()
    ));
    let mut app = app(dir.join("setting.json"));
    let state = app.new_entity(AppState::for_test);
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.update(|view, window, cx| {
        view.open_settings_section(SettingsSection::Shortcuts, window, cx);
        view.settings.shortcut_query = "Command center".into();
    });
    for read_only in [true, false] {
        app.update(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.clone();
            owner.update(cx, |owner, cx| {
                owner.read_only = read_only;
                owner.saving = !read_only;
                cx.notify();
            });
        });
        window.draw();
        for id in [
            "shortcut-openCommandCenter",
            "add-openCommandCenter",
            "remove-openCommandCenter-0",
            "disable-openCommandCenter",
            "reset-openCommandCenter",
        ] {
            let point = app.read(|cx| cx.global::<TestTargets>().0[id].center());
            window.simulate_click(point, MouseButton::Left);
        }
        window.simulate_keystroke("ctrl-y");
        app.read(|cx| {
            let owner = cx.global::<PreferenceOwner>().0.read(cx);
            assert_eq!(
                owner.snapshot.shortcut_bindings.as_ref().unwrap()["openCommandCenter"],
                ["Ctrl+q", "Ctrl+w"]
            );
            assert!(owner.error.is_none());
        });
    }
    assert!(!dir.exists());
}
