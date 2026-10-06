use crate::app::store::AppState;
use crate::backend::plugin_config::ConfigAction;
use crate::shared::plugin_config::{ConfigEdit, ConfigScope};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn receive(rx: &std::sync::mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}
fn respond(
    s: &mut AppState,
    key: &str,
    request: &Value,
    result: Value,
    cx: &mut gpui::Context<AppState>,
) {
    s.handle_response(key, request["id"].as_u64().unwrap(), Some(result), None, cx);
}
fn seed(
    s: &mut AppState,
    cx: &mut gpui::Context<AppState>,
) -> (String, std::sync::mpsc::Receiver<String>, String) {
    let (tx, rx) = std::sync::mpsc::channel();
    let key = s.workspaces[0].key.clone();
    s.workspaces[0].inbound = Some(tx);
    s.workspaces[0].started = true;
    s.active_workspace = Some(key.clone());
    s.workspaces[0].inspection.plugins.finish(crate::shared::plugins::PluginsOverviewResult::from_value(&json!({"marketplaces":[],"availablePlugins":[],"installedPlugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true}],"restorableBuiltins":[],"diagnostics":[],"capability":{"supported":true}})));
    s.open_plugin_config(&key, "fixture@local", ConfigScope::Workspace, cx);
    let get = receive(&rx);
    assert_eq!(get["params"]["configScope"], "workspace");
    respond(
        s,
        &key,
        &get,
        crate::shared::plugin_config::tests::fixture(),
        cx,
    );
    let token = s.workspaces[0]
        .inspection
        .plugin_config
        .as_ref()
        .unwrap()
        .token
        .clone();
    (key, rx, token)
}
#[gpui::test]
fn scoped_plugin_config_stale_preflight_requires_reload_and_keeps_edits(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s,cx| {
        let (key,rx,token) = seed(s,cx);
        s.edit_plugin_config(&key,&token,"count",ConfigEdit::Text("7".into()),cx);
        s.submit_plugin_config(&key,&token,ConfigAction::Save,cx);
        s.submit_plugin_config(&key,&token,ConfigAction::Save,cx);
        let get = receive(&rx); assert!(rx.try_recv().is_err());
        let mut stale = crate::shared::plugin_config::tests::fixture(); stale["plugins"][0]["configuredOptions"]["count"] = json!(6);
        respond(s,&key,&get,stale,cx);
        s.submit_plugin_config(&key,&token,ConfigAction::Save,cx); assert!(rx.try_recv().is_err());
        assert!(s.workspaces[0].inspection.plugin_config.as_ref().unwrap().needs_reload);
        s.read_plugin_config(&key,true,cx); let get = receive(&rx);
        respond(s,&key,&get,crate::shared::plugin_config::tests::fixture(),cx);
        assert!(!s.workspaces[0].inspection.plugin_config.as_ref().unwrap().edits.is_empty());
        s.submit_plugin_config(&key,&token,ConfigAction::Save,cx); let get = receive(&rx);
        respond(s,&key,&get,crate::shared::plugin_config::tests::fixture(),cx);
        let write = receive(&rx); assert_eq!(write["method"], "plugins/configure"); assert_eq!(write["params"]["options"], json!({"count":7.0})); assert_eq!(write["params"]["scope"], "workspace");
        s.edit_plugin_config(&key,&token,"count",ConfigEdit::Text("8".into()),cx);
        respond(s,&key,&write,json!({"pluginId":"fixture@local","diagnostics":[]}),cx);
        assert!(matches!(&s.workspaces[0].inspection.plugin_config.as_ref().unwrap().edits["count"].0,ConfigEdit::Text(v) if v == "8"));
        assert_eq!(receive(&rx)["method"], "plugins/overview");
        let refresh = receive(&rx); assert_eq!(refresh["method"], "plugins/list");
        respond(s,&key,&refresh,crate::shared::plugin_config::tests::fixture(),cx);
        assert!(matches!(&s.workspaces[0].inspection.plugin_config.as_ref().unwrap().edits["count"].0,ConfigEdit::Text(v) if v == "8"));
    });
}
#[gpui::test]
fn scoped_reset_confirms_and_uncertain_outcome_does_not_replay(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx, token) = seed(s, cx);
        s.submit_plugin_config(&key, &token, ConfigAction::Reset, cx);
        assert!(rx.try_recv().is_err());
        s.confirm_plugin_reset(&key, &token, true, cx);
        s.confirm_plugin_reset(&key, &token, false, cx);
        s.submit_plugin_config(&key, &token, ConfigAction::Reset, cx);
        assert!(rx.try_recv().is_err());
        s.confirm_plugin_reset(&key, &token, true, cx);
        s.submit_plugin_config(&key, &token, ConfigAction::Reset, cx);
        let get = receive(&rx);
        respond(
            s,
            &key,
            &get,
            crate::shared::plugin_config::tests::fixture(),
            cx,
        );
        let write = receive(&rx);
        assert_eq!(write["method"], "plugins/resetConfig");
        assert_eq!(write["params"]["scope"], "workspace");
        assert!(write["params"].get("options").is_none());
        respond(
            s,
            &key,
            &write,
            json!({"pluginId":"other","diagnostics":[]}),
            cx,
        );
        s.confirm_plugin_reset(&key, &token, true, cx);
        s.submit_plugin_config(&key, &token, ConfigAction::Reset, cx);
        assert!(rx.try_recv().is_err());
        assert!(
            s.workspaces[0]
                .inspection
                .plugin_config
                .as_ref()
                .unwrap()
                .needs_reload
        );
    });
}
#[gpui::test]
fn scoped_plugin_secret_write_receipt_has_versions_only_and_reconnect_retains_draft(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx, token) = seed(s, cx);
        s.edit_plugin_config(
            &key,
            &token,
            "token",
            ConfigEdit::Text("private-sentinel".into()),
            cx,
        );
        s.submit_plugin_config(&key, &token, ConfigAction::Save, cx);
        let get = receive(&rx);
        respond(
            s,
            &key,
            &get,
            crate::shared::plugin_config::tests::fixture(),
            cx,
        );
        let write = receive(&rx);
        assert_eq!(
            write["params"]["options"],
            json!({"token":"private-sentinel"})
        );
        s.workspaces[0].invalidate_connection();
        let form = s.workspaces[0].inspection.plugin_config.as_ref().unwrap();
        assert!(form.needs_reload);
        assert!(form.edits.contains_key("token"));
        assert!(!s.plugin_config_pending(&key));
        respond(
            s,
            &key,
            &write,
            json!({"pluginId":"fixture@local","diagnostics":[]}),
            cx,
        );
        assert!(
            s.workspaces[0]
                .inspection
                .plugin_config
                .as_ref()
                .unwrap()
                .edits
                .contains_key("token")
        );
        assert!(rx.try_recv().is_err());
    });
}
