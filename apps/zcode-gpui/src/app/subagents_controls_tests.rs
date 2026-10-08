use crate::app::subagent_profiles::AgentsListResult;
use crate::app::{root::RootView, settings::SettingsSection, store::AppState, test_support::*};
use crate::shared::{
    isolation::IsolatedSettings,
    preferences::{PreferenceOwner, Preferences},
    settings::AppSettings,
};
use gpui::{MouseButton, TestApp, px, size};
use serde_json::json;
use std::sync::Arc;

fn fixture() -> (TestApp, gpui::Entity<AppState>, Arc<IsolatedSettings>) {
    let isolated = IsolatedSettings::create().unwrap();
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            AppSettings::default(),
            isolated.home().join("setting.json"),
            Arc::new(|| false),
            cx,
        );
        let owner = cx.global::<PreferenceOwner>().0.clone();
        owner.update(cx, |owner, _| owner.read_only = true);
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |state, cx| {
        state.profiles.isolated = Some(isolated.clone());
        state.profiles.starting = true;
        state.profiles.models_loading = true;
        let snapshot: AgentsListResult = serde_json::from_value(json!({"agents":[],"userAgents":[],"pluginAgents":[],"capability":{"userScopeAvailable":true}})).unwrap();
        state.profiles.queries.entry("user".into()).or_default().snapshot = Some(snapshot);
        state.composer.update(cx, |input, _| input.set_text("parent draft"));
    });
    (app, state, isolated)
}

fn center(app: &TestApp, id: &str) -> gpui::Point<gpui::Pixels> {
    app.read(|cx| cx.global::<TestTargets>().0[id].center())
}

#[test]
fn measured_new_form_validates_and_captures_origin_without_competing_writer() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, isolated) = fixture();
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings_section(SettingsSection::Subagents, window, cx);
        view
    });
    window.simulate_resize(size(px(1280.), px(900.)));
    window.draw();
    window.simulate_click(center(&app, "subagents-new"), MouseButton::Left);
    window.read(|view, _| assert!(view.subagents.form.is_some()));
    window.draw();
    window.simulate_click(center(&app, "profile-form-save"), MouseButton::Left);
    window.read(|view, cx| {
        assert!(view.subagents.form.as_ref().unwrap().error.is_some());
        assert!(!view.state.read(cx).profiles.mutation_pending);
    });
    for (id, text) in [
        ("profile-input-name", "new-agent"),
        ("profile-input-description", "Description"),
        ("profile-input-prompt", "Prompt line one\nPrompt line two"),
    ] {
        window.draw();
        window.simulate_click(center(&app, id), MouseButton::Left);
        window.simulate_input(text);
    }
    window.draw();
    window.simulate_click(center(&app, "profile-form-save"), MouseButton::Left);
    window.read(|view, cx| {
        let state = view.state.read(cx);
        assert!(state.profiles.mutation_pending);
        assert_eq!(state.profiles.queue.len(), 1);
        let crate::app::profile_state::ProfileRequest::Preflight(mutation) =
            state.profiles.queue.front().unwrap()
        else {
            panic!("expected preflight")
        };
        assert_eq!(mutation.scope, "user");
        assert_eq!(mutation.params["config"]["name"], "new-agent");
        assert_eq!(
            mutation.params["config"]["systemPrompt"],
            "Prompt line one\nPrompt line two"
        );
        assert_eq!(state.composer.read(cx).text(), "parent draft");
    });
    window.simulate_click(center(&app, "profile-form-back"), MouseButton::Left);
    window.read(|view, cx| {
        assert!(view.subagents.form.is_none());
        assert!(view.state.read(cx).profiles.mutation_pending);
    });
    assert!(!isolated.home().join("setting.json").exists());
}

#[test]
fn failed_inventory_is_not_retried_from_render_and_old_form_id_cannot_submit() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, state, _) = fixture();
    app.update_entity(&state, |s, _| {
        let query = s.profiles.queries.get_mut("user").unwrap();
        query.snapshot = None;
        query.error = Some("Failed once".into());
    });
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings_section(SettingsSection::Subagents, window, cx);
        view
    });
    window.draw();
    window.draw();
    app.read_entity(&state, |s, _| assert!(s.profiles.queue.is_empty()));
    window.update(|view, window, cx| {
        view.open_profile_form(None, false, window, cx);
        let old = view.subagents.form.as_ref().unwrap().id.clone();
        view.open_profile_form(None, false, window, cx);
        assert_ne!(old, view.subagents.form.as_ref().unwrap().id);
        view.save_profile_form(&old, false, cx);
        assert!(!view.state.read(cx).profiles.mutation_pending);
        assert!(view.subagents.form.as_ref().unwrap().error.is_none());
    });
}
