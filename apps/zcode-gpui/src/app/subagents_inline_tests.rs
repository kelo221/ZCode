use super::{
    root::RootView,
    settings::SettingsSection,
    store::AppState,
    subagent_profiles::{AgentSummary, AgentsListResult, ModelSelectionView},
    test_support::*,
};
use crate::shared::{isolation::IsolatedSettings, preferences::Preferences, settings::AppSettings};
use gpui::TestApp;
use serde_json::json;
use std::sync::Arc;

#[test]
fn inline_select_is_complete_single_lane_and_stale_scope_cannot_submit() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let isolated = IsolatedSettings::create().unwrap();
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        Preferences::install_at(
            AppSettings::default(),
            isolated.home().join("setting.json"),
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let agent: AgentSummary = serde_json::from_value(json!({"id":"builtin:Explore","name":"Explore","description":"Read-only","systemPrompt":"p","path":"builtin:Explore","scope":"built-in","source":"built-in","enabled":true})).unwrap();
    app.update_entity(&state, |s, _| {
        s.profiles.isolated = Some(isolated.clone()); s.profiles.starting = true;
        s.profiles.queries.entry("user".into()).or_default().snapshot = Some(serde_json::from_value::<AgentsListResult>(json!({"agents":[agent],"userAgents":[],"pluginAgents":[],"capability":{"userScopeAvailable":true}})).unwrap());
        s.profiles.models = Some(serde_json::from_value::<ModelSelectionView>(json!({"revision":1,"providers":[{"providerId":"p","providerName":"Provider","templateId":"t","config":{"api":{"type":"openai-responses"}},"models":[{"modelId":"m","config":{"optionSpecs":{"reasoningLevel":{"values":["low","high"]}}}}]}]})).unwrap());
    });
    let mut window = app.open_window(|window, cx| {
        let mut view = RootView::new(state.clone(), cx);
        view.open_settings_section(SettingsSection::Subagents, window, cx);
        view
    });
    window.update(|view, _, cx| {
        view.submit_inline_selection("other", &agent, "p/m", false, cx);
        assert!(view.state.read(cx).profiles.queue.is_empty());
        view.submit_inline_selection("user", &agent, "p/m", false, cx);
        let s = view.state.read(cx);
        assert!(s.profiles.mutation_pending);
        let super::profile_state::ProfileRequest::Preflight(m) = s.profiles.queue.front().unwrap()
        else {
            panic!("preflight");
        };
        assert_eq!(
            m.params["modelSelection"]["options"]["reasoningLevel"],
            "high"
        );
        assert!(view.subagents.form.is_none());
        view.submit_inline_selection("user", &agent, "", false, cx);
        assert_eq!(view.state.read(cx).profiles.queue.len(), 1);
    });
}
