use crate::app::store::AppState;
use crate::backend::{plugin_detail::PluginDetailIdentity, workspace::WorkspaceHandle};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn plugin_description_reply_populates_only_origin_after_workspace_switch(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx,|s,cx| {
        let (tx,rx) = std::sync::mpsc::channel(); let a = s.workspaces[0].key.clone(); s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true; s.active_workspace = Some(a.clone());
        let overview = json!({"marketplaces":[],"installedPlugins":[],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true},"availablePlugins":[{"id":"p","name":"plugin","marketplace":"m","installed":false}]});
        s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&overview));
        let identity = PluginDetailIdentity {id:"p".into(),name:"plugin".into(),marketplace:"m".into()}; s.open_plugin_detail(&a,identity.clone(),cx);
        let get: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        let mut b = WorkspaceHandle::new(std::env::temp_dir().join("describe-owner-b"),vec![]); let b_key = b.key.clone(); b.inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&overview)); s.workspaces.push(b); s.active_workspace = Some(b_key);
        s.handle_response(&a,get["id"].as_u64().unwrap(),Some(json!({"components":[]})),None,cx);
        assert!(s.workspaces[0].inspection.plugin_descriptions[&identity.cache_key()].value.is_some()); assert!(s.workspaces[1].inspection.plugin_descriptions.is_empty()); assert!(s.workspaces[1].inspection.plugin_detail.is_none());
    });
}
