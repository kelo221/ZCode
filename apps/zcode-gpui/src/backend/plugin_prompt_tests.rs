use crate::app::store::AppState;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn overview(installed: bool) -> crate::shared::plugins::PluginsOverviewResult {
    crate::shared::plugins::PluginsOverviewResult::from_value(
        &json!({"marketplaces":[],"availablePlugins":[{"id":"tool","name":"tool","marketplace":"personal","installed":installed}],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}}),
    )
}
fn catalog() -> Value {
    json!({"authority":"workspace","plugins":[{"pluginId":"tool","name":"tool","marketplace":"personal","enabled":true,"conflictingPluginIds":[],"skillQualifiedNames":[],"mcpServerNames":[],"subagentNames":[]}]})
}
#[gpui::test]
fn plugin_prompt_saves_parent_draft_and_inserts_reference_without_submission(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].inspection.plugins.finish(overview(true));
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.draft = false;
        s.composer.update(cx, |c, _| c.set_text("parent draft"));
        s.use_plugin_prompt(&key, "tool", "example", cx);
        s.use_plugin_prompt(&key, "tool", "example", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "plugins/referenceCatalog");
        assert!(request["params"].get("sessionId").is_none());
        assert!(rx.try_recv().is_err());
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(catalog()),
            None,
            cx,
        );
        assert!(rx.try_recv().is_err());
        assert!(s.active.is_none());
        assert!(s.draft);
        assert_eq!(s.session_drafts["parent"], "parent draft");
        assert_eq!(s.composer.read(cx).text(), "[@tool](plugin://tool) example");
        assert_eq!(s.composer.read(cx).caret, s.composer.read(cx).text().len());
        s.select_session(&key, "parent", cx);
        assert_eq!(s.composer.read(cx).text(), "parent draft");
    });
}
#[gpui::test]
fn plugin_prompt_blocks_existing_draft_and_late_edits_install_does_not_prefill(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].inspection.plugins.finish(overview(true));
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.session_drafts
            .insert(format!("draft:{key}"), "unsent draft".into());
        s.use_plugin_prompt(&key, "tool", "example", cx);
        assert!(rx.try_recv().is_err());
        s.session_drafts.clear();
        s.use_plugin_prompt(&key, "tool", "example", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.composer.update(cx, |c, _| c.set_text("newer edit"));
        s.handle_response(
            &key,
            request["id"].as_u64().unwrap(),
            Some(catalog()),
            None,
            cx,
        );
        assert_eq!(s.active.as_deref(), Some("parent"));
        assert_eq!(s.composer.read(cx).text(), "newer edit");
        s.workspaces[0].inspection.plugins.finish(overview(false));
        s.use_plugin_prompt(&key, "tool", "example", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "plugins/install");
        assert_eq!(s.composer.read(cx).text(), "newer edit");
        assert_eq!(s.active.as_deref(), Some("parent"));
    });
}
