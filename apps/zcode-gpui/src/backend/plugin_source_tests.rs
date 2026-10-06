use crate::app::store::AppState;
use crate::backend::workspace::WorkspaceHandle;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

fn ready(s: &mut AppState) -> (String, std::sync::mpsc::Receiver<String>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let ws = &mut s.workspaces[0];
    ws.key = "isolated-workspace-identity".into();
    ws.inbound = Some(tx);
    ws.started = true;
    ws.inspection
        .plugins
        .finish(crate::shared::plugins::PluginsOverviewResult {
            capability_supported: true,
            ..Default::default()
        });
    s.active_workspace = Some(ws.key.clone());
    (ws.key.clone(), rx)
}

#[gpui::test]
fn plugin_mutations_share_owner_pending_and_file_identity_boundary(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.install_plugin_for(&key, "tool", "personal", cx);
        s.refresh_plugin_sources_for(&key, cx);
        s.set_plugin_enabled_for(&key, "tool", false, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "plugins/install");
        assert_eq!(request["params"]["workspace"]["workspaceKey"], key);
        assert_eq!(
            request["params"]["workspace"]["workspacePath"],
            s.workspaces[0].path.to_string_lossy().as_ref()
        );
        assert!(rx.try_recv().is_err());
        s.active_workspace = Some("other".into());
        s.workspaces[0].pending.clear();
        s.uninstall_plugin_for(&key, "tool", "personal", cx);
        s.update_plugin_for(&key, "tool", "personal", cx);
        s.restore_builtin_plugin_for(&key, "tool", cx);
        s.remove_plugin_marketplace_for(&key, "personal", cx);
        assert!(rx.try_recv().is_err());
    });
}

#[gpui::test]
fn source_draft_survives_success_failure_and_reconnect(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        let input = s.source_input(cx).unwrap();
        input.update(cx, |c, _| c.set_text("https://example.invalid/source"));
        s.add_plugin_source_for(&key, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "plugins/marketplace/add");
        input.update(cx, |c, _| c.set_text("newer draft"));
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"marketplace":{"id":"personal","name":"Personal","source":{},"pluginCount":0},"diagnostics":[]})), None, cx);
        assert_eq!(input.read(cx).text(), "newer draft");
        assert_eq!(serde_json::from_str::<Value>(&rx.try_recv().unwrap()).unwrap()["method"], "plugins/overview");
        s.workspaces[0].invalidate_connection();
        assert_eq!(s.workspaces[0].plugin_source_draft.input.as_ref().unwrap().read(cx).text(), "newer draft");
        assert!(!s.log.iter().any(|l| l.contains("example.invalid")));
    });
}

#[gpui::test]
fn plugin_error_diagnostics_and_transport_failures_stay_with_origin(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.refresh_plugin_sources_for(&key, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        let b = WorkspaceHandle::new(std::env::temp_dir().join("source-other"), vec![]);
        s.active_workspace = Some(b.key.clone());
        s.workspaces.push(b);
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"marketplaces":[],"diagnostics":[{"code":"refresh","severity":"error","message":"token=sentinel-secret"}]})), None, cx);
        let error = s.ws(&key).unwrap().inspection.plugin_operation_error.as_ref().unwrap();
        assert!(!error.contains("sentinel-secret"));
        assert!(s.active_inspection().unwrap().plugin_operation_error.is_none());
        assert!(s.ws(&key).unwrap().inspection.plugins.value.is_some());
        assert!(rx.try_recv().is_err());
        s.active_workspace = Some(key.clone());
        s.refresh_plugin_sources_for(&key, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, request["id"].as_u64().unwrap(), None, Some(json!({"message":"token=another-secret"})), cx);
        assert!(!s.ws(&key).unwrap().inspection.plugin_operation_error.as_ref().unwrap().contains("another-secret"));
    });
}

#[gpui::test]
fn plugin_failed_enqueue_unsupported_and_missing_results_do_not_complete(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = ready(s);
        s.workspaces[0]
            .inspection
            .plugins
            .value
            .as_mut()
            .unwrap()
            .capability_supported = false;
        s.refresh_plugin_sources_for(&key, cx);
        assert!(rx.try_recv().is_err());
        s.workspaces[0]
            .inspection
            .plugins
            .value
            .as_mut()
            .unwrap()
            .capability_supported = true;
        s.refresh_plugin_sources_for(&key, cx);
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, request["id"].as_u64().unwrap(), None, None, cx);
        assert!(s.workspaces[0].inspection.plugin_operation_error.is_some());
        drop(rx);
        s.refresh_plugin_sources_for(&key, cx);
        assert!(!s.plugin_action_pending(&key));
        assert!(s.workspaces[0].inspection.plugin_operation_error.is_some());
    });
}

#[gpui::test]
fn source_picker_cancel_edit_and_navigation_never_replace_draft(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, _) = ready(s);
        let input = s.source_input(cx).unwrap();
        input.update(cx, |c, _| c.set_text("original"));
        let generation = s.navigation_generation;
        s.settle_plugin_source_pick(&key, generation, &input, "original", Ok(None), cx);
        assert_eq!(input.read(cx).text(), "original");
        input.update(cx, |c, _| c.set_text("edited"));
        s.settle_plugin_source_pick(
            &key,
            generation,
            &input,
            "original",
            Ok(Some("/picked".into())),
            cx,
        );
        assert_eq!(input.read(cx).text(), "edited");
        s.navigation_generation += 1;
        s.settle_plugin_source_pick(
            &key,
            generation,
            &input,
            "edited",
            Ok(Some("/picked".into())),
            cx,
        );
        assert_eq!(input.read(cx).text(), "edited");
        s.settle_plugin_source_pick(
            &key,
            s.navigation_generation,
            &input,
            "edited",
            Ok(Some("/picked".into())),
            cx,
        );
        assert_eq!(input.read(cx).text(), "/picked");
    });
}
