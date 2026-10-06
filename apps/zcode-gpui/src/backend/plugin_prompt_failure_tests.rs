use crate::app::store::AppState;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn unavailable_prompt_reference_cannot_prefill_and_errors_are_sanitized(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].started = true; s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[{"id":"tool","name":"tool","marketplace":"personal","installed":true}],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        s.active_workspace = Some(key.clone()); s.active = Some("parent".into()); s.draft = false;
        s.composer.update(cx, |c, _| c.set_text("parent"));
        for (enabled, conflicts) in [(false, json!([])), (true, json!(["other"]))] {
            s.use_plugin_prompt(&key, "tool", "Example", cx);
            let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert!(!s.plugin_operation_available(&key));
            s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"authority":"workspace","plugins":[{"pluginId":"tool","name":"tool","marketplace":"personal","enabled":enabled,"conflictingPluginIds":conflicts,"skillQualifiedNames":[],"mcpServerNames":[]}]})), None, cx);
            assert_eq!(s.active.as_deref(), Some("parent"));
            assert_eq!(s.composer.read(cx).text(), "parent");
            assert!(s.workspaces[0].inspection.plugin_operation_error.is_some());
        }
        s.use_plugin_prompt(&key, "tool", "Example", cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, request["id"].as_u64().unwrap(), None, Some(json!({"message":"token=secret"})), cx);
        assert!(!s.workspaces[0].inspection.plugin_operation_error.as_ref().unwrap().contains("secret"));
        s.use_plugin_prompt(&key, "tool", "Example", cx);
        assert!(rx.try_recv().is_ok());
        s.workspaces[0].invalidate_connection();
        assert!(!s.plugin_prompt_pending(&key));
        assert_eq!(s.composer.read(cx).text(), "parent");
    });
}
