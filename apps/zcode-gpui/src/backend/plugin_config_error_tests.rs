use crate::app::store::AppState;
use crate::shared::plugin_config::ConfigScope;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn plugin_config_rpc_errors_never_retain_unlabelled_credentials(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx,|s,cx| {
        let key = s.workspaces[0].key.clone(); let (tx,rx) = std::sync::mpsc::channel(); s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true; s.active_workspace = Some(key.clone());
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        s.open_plugin_config(&key,"fixture@local",ConfigScope::User,cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key,request["id"].as_u64().unwrap(),None,Some(json!({"message":"unlabelled-private-sentinel"})),cx);
        let form = s.workspaces[0].inspection.plugin_config.as_ref().unwrap(); assert!(form.needs_reload); assert!(!form.error.as_ref().unwrap().contains("unlabelled-private-sentinel"));
        assert!(!s.workspaces[0].status.contains("unlabelled-private-sentinel")); assert!(s.errors.iter().all(|s|!s.contains("unlabelled-private-sentinel"))); assert!(s.log.iter().all(|s|!s.contains("unlabelled-private-sentinel")));
        let previous_errors = s.errors.len(); let previous_status = s.workspaces[0].status.clone();
        s.handle_response(&key,request["id"].as_u64().unwrap(),None,Some(json!({"message":"late-private-sentinel"})),cx);
        assert_eq!(s.errors.len(),previous_errors); assert_eq!(s.workspaces[0].status,previous_status);
    });
}
