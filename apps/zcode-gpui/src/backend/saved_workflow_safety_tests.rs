use crate::app::store::AppState;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn refreshed_argument_declarations_require_reselection_and_interruption_keeps_draft(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        let key = ws.key.clone();
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        let mut value = json!({"workflows":[{"name":"test","description":"Test","scope":"project","path":"/test","args":{"value":{"type":"json","required":true,"default":null}}}],"invalid":[],"dir":"/dir"});
        s.settle_saved_workflow_list(&key, "project", Ok(value.clone()));
        s.select_saved_workflow(&key, "project", "test", cx);
        s.launch_saved_workflow(&key, "project", "test", cx);
        let create: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, create["id"].as_u64().unwrap(), Some(json!({"status":"rejected"})), None, cx);
        value["workflows"][0]["args"]["value"]["type"] = json!("string");
        value["workflows"][0]["args"]["value"]["default"] = json!("default");
        s.settle_saved_workflow_list(&key, "project", Ok(value));
        s.launch_saved_workflow(&key, "project", "test", cx);
        assert!(rx.try_recv().is_err());
        assert!(s.workspaces[0].saved_workflow_form.error.as_deref().unwrap().contains("changed"));
        s.select_saved_workflow(&key, "project", "test", cx);
        let field = s.workspaces[0].saved_workflow_form.inputs["value"].clone();
        field.update(cx, |c, _| c.set_text("retained"));
        s.launch_saved_workflow(&key, "project", "test", cx);
        assert!(rx.try_recv().is_ok());
        s.workspaces[0].invalidate_connection();
        assert_eq!(field.read(cx).text(), "retained");
        assert!(s.workspaces[0].saved_workflow_form.error.as_deref().unwrap().contains("unknown"));
        assert!(!s.saved_workflow_launch_pending(&key));
    });
}

#[gpui::test]
fn launch_error_cannot_annotate_a_new_form_generation(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let launch = crate::backend::saved_workflow_cmds::SavedWorkflowLaunch {
            generation: 0,
            name: "test".into(),
            scope: "project".into(),
            args: json!({}),
            form_generation: "old".into(),
        };
        s.saved_workflow_launch_error(&key, &launch, "token=secret");
        assert!(s.workspaces[0].saved_workflow_form.error.is_none());
        assert!(!s.workspaces[0].status.contains("secret"));
        s.workspaces[0].saved_workflow_form.generation = "old".into();
        s.saved_workflow_launch_error(&key, &launch, "token=secret");
        assert!(s.workspaces[0].saved_workflow_form.error.is_some());
        assert!(
            !s.workspaces[0]
                .saved_workflow_form
                .error
                .as_ref()
                .unwrap()
                .contains("secret")
        );
        cx.notify();
    });
}
