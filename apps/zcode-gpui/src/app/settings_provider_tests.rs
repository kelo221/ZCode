use crate::app::settings::SettingsSection;
use crate::app::{root::RootView, store::AppState, test_support::*};
use crate::shared::preferences::Preferences;
use crate::shared::settings::AppSettings;
use gpui::{MouseButton, Pixels, Point, TestApp, px, size};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

fn app(path: std::path::PathBuf) -> TestApp {
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

fn target(app: &mut TestApp, id: &str) -> Point<Pixels> {
    app.read(|cx| {
        cx.global::<TestTargets>()
            .0
            .get(id)
            .unwrap_or_else(|| panic!("target {id} not rendered"))
            .center()
    })
}

fn sent(rx: &Receiver<String>) -> Vec<Value> {
    rx.try_iter()
        .map(|line| serde_json::from_str(&line).unwrap())
        .collect()
}

#[test]
fn provider_manager_lists_selects_and_sends_single_field_edits() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-provider-ui-{}", uuid::Uuid::now_v7()));
    let mut app = app(dir.join("setting.json"));
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    let key = app.update_entity(&state, |s, _| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key.clone());
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(900.)));
    window
        .update(|view, window, cx| view.open_settings_section(SettingsSection::Models, window, cx));
    let read = sent(&rx);
    assert_eq!(
        read.len(),
        1,
        "entering Models reads the provider view once"
    );
    assert_eq!(read[0]["method"], "providerSettings/read");
    let view = crate::provider_settings::tests::sample_view();
    app.update_entity(&state, |s, cx| {
        let id = read[0]["id"].as_u64().unwrap();
        s.handle_response(&key, id, Some(json!({ "view": view })), None, cx);
    });
    window.draw();

    // Presets first: the account provider is selected and has no key or switch.
    for id in [
        "provider-nav-zai",
        "provider-nav-c1",
        "provider-nav-c2",
        "provider-add",
    ] {
        target(&mut app, id);
    }
    app.read(|cx| {
        let targets = &cx.global::<TestTargets>().0;
        assert!(!targets.contains_key("provider-api-key"));
        assert!(!targets.contains_key("provider-enabled"));
    });

    let c1 = target(&mut app, "provider-nav-c1");
    window.simulate_click(c1, MouseButton::Left);
    window.draw();
    let key_field = target(&mut app, "provider-api-key");
    window.simulate_click(key_field, MouseButton::Left);
    window.simulate_keystroke("ctrl-a");
    window.simulate_input("sk-new");
    window.simulate_keystroke("enter");
    let edit = sent(&rx);
    assert_eq!(edit.len(), 1);
    assert_eq!(edit[0]["method"], "providerSettings/mutate");
    assert_eq!(
        edit[0]["params"]["operation"],
        json!({ "type": "updateProvider", "providerId": "c1", "apiKey": "sk-new" })
    );
    window.read(|view, cx| {
        assert!(view.state.read(cx).provider_settings.busy);
        // Agent replied nothing yet: the draft stays and nothing was written locally.
        let form = view.settings.provider_form.as_ref().unwrap();
        assert_eq!(form.api_key.read(cx).text(), "sk-new");
    });
    assert!(!dir.exists());
}

#[test]
fn delete_needs_confirmation_and_old_agents_get_the_read_only_catalog() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("gpui-provider-del-{}", uuid::Uuid::now_v7()));
    let mut app = app(dir.join("setting.json"));
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, _| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.provider_settings.view = crate::provider_settings::ProviderView::from_value(
            &crate::provider_settings::tests::sample_view(),
        );
        s.provider_settings.selected = Some("c2".into());
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(900.)));
    window
        .update(|view, window, cx| view.open_settings_section(SettingsSection::Models, window, cx));
    let _ = sent(&rx);
    app.update_entity(&state, |s, _| s.provider_settings.loading = false);
    window.draw();
    let delete = target(&mut app, "provider-delete");
    window.simulate_click(delete, MouseButton::Left);
    window.draw();
    assert!(
        sent(&rx).is_empty(),
        "first click only asks for confirmation"
    );
    let confirm = target(&mut app, "provider-delete-confirm");
    window.simulate_click(confirm, MouseButton::Left);
    let ops = sent(&rx);
    assert_eq!(
        ops[0]["params"]["operation"],
        json!({ "type": "deleteProvider", "providerId": "c2" })
    );

    app.update_entity(&state, |s, _| {
        s.provider_settings = Default::default();
        s.provider_settings.unavailable = true;
    });
    app.update(|cx| cx.set_global(TestTargets::default()));
    window.draw();
    target(&mut app, "settings-model-search");
    app.read(|cx| assert!(!cx.global::<TestTargets>().0.contains_key("provider-add")));
    assert!(!dir.exists());
}
