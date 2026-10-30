use crate::app::store::AppState;
use crate::backend::submission::Submission;
use crate::backend::workspace::Pending;
use crate::shared::settings::LastComposerModel;
use gpui::{AppContext, Context, TestAppContext};
use serde_json::json;

fn with_catalog(state: &mut AppState) -> String {
    let key = state.workspaces[0].key.clone();
    state.active_workspace = Some(key.clone());
    let mut catalog = crate::composer::catalog::WorkspaceConfig::default();
    catalog.apply_state(
        &json!({"configOptions":[{"id":"model","currentValue":"test/model","options":[
            {"value":"test/model","name":"Model","modelThoughtLevels":["high"]},
            {"value":"test/other","name":"Other","modelThoughtLevels":["low","max"]}
        ]}]}),
    );
    state.workspace_configs.insert(key.clone(), catalog);
    key
}

/// `select_session` without spawning a backend for the test workspace.
fn open_session(state: &mut AppState, sid: &str, cx: &mut Context<AppState>) {
    state.save_current_draft(cx);
    state.active = Some(sid.into());
    state.draft = false;
    state.ui_model_value = None;
    state.ui_mode = None;
    state.ui_plan = false;
    state.ui_thought = None;
    state.restore_draft(sid, cx);
}

#[gpui::test]
fn new_draft_keeps_its_model_and_thinking_across_navigation(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        with_catalog(state);
        state.ui_model_value = Some("test/other".into());
        state.ui_thought = Some("max".into());
        state.composer.update(cx, |c, _| c.set_text("draft text"));

        open_session(state, "old", cx);
        assert_eq!(
            state.ui_model_value, None,
            "an old thread never inherits draft picks"
        );
        assert_eq!(state.ui_thought, None);

        state.new_chat(cx);
        assert_eq!(state.composer.read(cx).text(), "draft text");
        assert_eq!(state.ui_model_value.as_deref(), Some("test/other"));
        assert_eq!(state.ui_thought.as_deref(), Some("max"));
        let effective = state.effective_config();
        assert_eq!(
            (effective.model.as_str(), effective.thought.as_str()),
            ("other", "max")
        );
    });
}

#[gpui::test]
fn fresh_draft_starts_from_the_last_used_model(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, cx| {
        let key = with_catalog(state);
        assert_eq!(state.effective_config().model, "model", "workspace default");

        state.ui_model_value = Some("test/other".into());
        state.ui_thought = Some("max".into());
        let submission = Submission {
            workspace: key.clone(),
            draft_key: format!("draft:{key}"),
            navigation_generation: state.navigation_generation,
            text: "hi".into(),
            attachments: vec![],
            config: serde_json::Value::Null,
            model_choice: Some(state.current_model_choice()),
        };
        state.workspaces[0]
            .pending
            .insert(1, Pending::CreateSession(submission));
        state.handle_response(
            &key,
            1,
            Some(json!({"status":"accepted","result":{"sessionId":"created"}})),
            None,
            cx,
        );
        assert_eq!(state.active.as_deref(), Some("created"));
        assert_eq!(
            state.draft_models.last,
            Some(LastComposerModel {
                provider: "test".into(),
                model: "other".into(),
                thought: "max".into(),
            })
        );

        state.active = None;
        state.draft = true;
        assert_eq!(state.ui_model_value, None);
        let effective = state.effective_config();
        assert_eq!(
            (effective.model.as_str(), effective.thought.as_str()),
            ("other", "max")
        );
        let config = state.draft_config();
        assert_eq!(config["model"], "other");
        assert_eq!(config["modelSelection"]["options"]["reasoningLevel"], "max");

        // Picking another model drops the remembered thinking level it does not offer.
        state.ui_model_value = Some("test/model".into());
        assert_eq!(state.effective_config().thought, "");
    });
}

#[gpui::test]
fn last_used_model_missing_from_the_catalog_falls_back_to_the_default(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |state, _| {
        with_catalog(state);
        state.draft_models.last = Some(LastComposerModel {
            provider: "gone".into(),
            model: "retired".into(),
            thought: "max".into(),
        });
        let effective = state.effective_config();
        assert_eq!(
            (effective.model.as_str(), effective.thought.as_str()),
            ("model", "")
        );
        assert!(state.draft_config().get("model").is_none());
    });
}
