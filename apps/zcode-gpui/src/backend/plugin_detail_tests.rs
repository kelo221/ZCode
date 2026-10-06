use crate::app::store::AppState;
use crate::backend::plugin_detail::PluginDetailIdentity;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn plugin_detail_is_scoped_read_only_and_late_reply_does_not_reopen(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel(); let key = s.workspaces[0].key.clone();
        s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true; s.active_workspace = Some(key.clone());
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true},"availablePlugins":[{"id":"p","name":"plugin","marketplace":"m","installed":false}]})));
        s.composer.update(cx, |c, _| c.set_text("kept"));
        let identity = PluginDetailIdentity { id: "p".into(), name: "plugin".into(), marketplace: "m".into() };
        s.open_plugin_detail(&key, identity.clone(), cx); s.open_plugin_detail(&key, identity.clone(), cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(get["method"], "plugins/describe"); assert_eq!(get["params"]["pluginName"], "plugin"); assert_eq!(get["params"]["workspace"]["workspaceKey"], key); assert!(rx.try_recv().is_err());
        s.close_plugin_detail(&key, &identity, cx);
        s.handle_response(&key, get["id"].as_u64().unwrap(), Some(json!({"components":[]})), None, cx);
        assert!(s.workspaces[0].inspection.plugin_detail.is_none()); assert_eq!(s.composer.read(cx).text(), "kept");
        s.open_plugin_detail(&key, identity.clone(), cx); assert!(rx.try_recv().is_err());
        s.fetch_plugin_description(&key, &identity, true, cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, get["id"].as_u64().unwrap(), None, Some(json!({"message":"token=secret"})), cx);
        let q = &s.workspaces[0].inspection.plugin_descriptions[&identity.cache_key()];
        assert!(q.value.is_some()); assert!(!q.error.as_ref().unwrap().contains("secret"));
        s.workspaces[0].invalidate_connection(); assert!(s.workspaces[0].inspection.plugin_detail.is_none()); assert!(s.workspaces[0].inspection.plugin_descriptions.is_empty());
    });
}
