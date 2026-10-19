use crate::app::root::RootView;
use crate::app::settings::SettingsSection;
use crate::app::store::AppState;
use crate::app::test_support::{RUNTIME_TEST_LOCK, TestTargets};
use crate::system_prompt::DefaultPrompt;
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

fn target(app: &TestApp, id: &str) -> gpui::Point<gpui::Pixels> {
    app.read(|cx| cx.global::<TestTargets>().0[id].center())
}

fn editor_text(window: &gpui::TestAppWindow<RootView>) -> String {
    window.read(|v, cx| {
        let editor = v.settings.system_prompt.as_ref().unwrap();
        editor.input.read(cx).text().to_owned()
    })
}

#[test]
fn editor_prefills_the_default_and_apply_reset_commit_through_the_owner() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        let path = std::env::temp_dir().join(format!("gpui-prompt-{}.json", uuid::Uuid::now_v7()));
        crate::shared::preferences::Preferences::install_at(
            crate::shared::settings::load_settings_from_path(&path),
            path,
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        s.system_prompt.default_prompt = Some(DefaultPrompt {
            text: "Default body".into(),
            from_session: true,
        });
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    window.simulate_click(target(&app, "open-settings"), MouseButton::Left);
    window.draw();
    window.simulate_click(target(&app, "settings-system-prompt"), MouseButton::Left);
    window.read(|v, _| assert!(v.settings.section == SettingsSection::SystemPrompt));
    window.draw();
    assert_eq!(editor_text(&window), "Default body");

    // Untouched default text is not an override.
    window.simulate_click(
        target(&app, "settings-system-prompt-apply"),
        MouseButton::Left,
    );
    app.read_entity(&state, |s, _| {
        assert_eq!(s.system_prompt.override_text, None)
    });

    window.update(|v, _, cx| {
        let input = v.settings.system_prompt.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| input.set_text("Talk like a pirate.", cx));
    });
    window.draw();
    window.simulate_click(
        target(&app, "settings-system-prompt-apply"),
        MouseButton::Left,
    );
    app.read_entity(&state, |s, _| {
        assert_eq!(
            s.system_prompt.override_text.as_deref(),
            Some("Talk like a pirate.")
        )
    });
    window.draw();
    assert_eq!(editor_text(&window), "Talk like a pirate.");

    window.simulate_click(
        target(&app, "settings-system-prompt-reset"),
        MouseButton::Left,
    );
    app.read_entity(&state, |s, _| {
        assert_eq!(s.system_prompt.override_text, None)
    });
    window.draw();
    assert_eq!(editor_text(&window), "Default body");
}
