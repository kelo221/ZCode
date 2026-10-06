use crate::app::store::AppState;
use crate::backend::plugin_config::ConfigAction;
use crate::backend::workspace::WorkspaceHandle;
use crate::shared::plugin_config::{ConfigEdit, ConfigScope};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn config_write_after_close_and_workspace_switch_refreshes_origin_only(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx,|s,cx| {
        let a = s.workspaces[0].key.clone(); let (tx,rx) = std::sync::mpsc::channel(); s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true; s.active_workspace = Some(a.clone());
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        s.open_plugin_config(&a,"fixture@local",ConfigScope::User,cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&a,get["id"].as_u64().unwrap(),Some(crate::shared::plugin_config::tests::fixture()),None,cx);
        let token = s.workspaces[0].inspection.plugin_config.as_ref().unwrap().token.clone();
        s.edit_plugin_config(&a,&token,"count",ConfigEdit::Text("9".into()),cx); s.submit_plugin_config(&a,&token,ConfigAction::Save,cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&a,get["id"].as_u64().unwrap(),Some(crate::shared::plugin_config::tests::fixture()),None,cx);
        let write: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap(); assert_eq!(write["params"]["scope"],"user");
        s.close_plugin_config(&a,&token,cx);
        let mut b = WorkspaceHandle::new(std::env::temp_dir().join("config-owner-b"),vec![]); let b_key = b.key.clone(); let (tx_b,rx_b) = std::sync::mpsc::channel(); b.inbound = Some(tx_b); b.started = true; s.workspaces.push(b); s.active_workspace = Some(b_key);
        s.handle_response(&a,write["id"].as_u64().unwrap(),Some(json!({"pluginId":"fixture@local","diagnostics":[]})),None,cx);
        let refresh: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap(); assert_eq!(refresh["method"],"plugins/overview"); assert_eq!(refresh["params"]["workspace"]["workspaceKey"],a); assert!(rx_b.try_recv().is_err());
        let scoped: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap(); assert_eq!(scoped["method"],"plugins/list"); assert_eq!(scoped["params"]["configScope"],"user"); assert_eq!(scoped["params"]["workspace"]["workspaceKey"],a); assert!(rx_b.try_recv().is_err());
        assert!(!s.workspaces[0].inspection.plugin_config.as_ref().unwrap().visible); assert!(s.workspaces[1].inspection.plugin_config.is_none());
    });
}

#[gpui::test]
fn edits_during_preflight_are_not_written_or_discarded_and_blank_secret_does_not_send(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx,|s,cx| {
        let key = s.workspaces[0].key.clone(); let (tx,rx) = std::sync::mpsc::channel(); s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true; s.active_workspace = Some(key.clone());
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
        s.open_plugin_config(&key,"fixture@local",ConfigScope::Workspace,cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap(); s.handle_response(&key,get["id"].as_u64().unwrap(),Some(crate::shared::plugin_config::tests::fixture()),None,cx);
        let token = s.workspaces[0].inspection.plugin_config.as_ref().unwrap().token.clone();
        s.edit_plugin_config(&key,&token,"token",ConfigEdit::Text(String::new()),cx); s.submit_plugin_config(&key,&token,ConfigAction::Save,cx); assert!(rx.try_recv().is_err());
        s.edit_plugin_config(&key,&token,"count",ConfigEdit::Text("9".into()),cx); s.submit_plugin_config(&key,&token,ConfigAction::Save,cx); let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.edit_plugin_config(&key,&token,"count",ConfigEdit::Text("10".into()),cx);
        s.handle_response(&key,get["id"].as_u64().unwrap(),Some(crate::shared::plugin_config::tests::fixture()),None,cx); assert!(rx.try_recv().is_err());
        assert!(matches!(&s.workspaces[0].inspection.plugin_config.as_ref().unwrap().edits["count"].0,ConfigEdit::Text(v) if v == "10"));
    });
}
