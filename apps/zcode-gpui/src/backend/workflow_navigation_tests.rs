use crate::{
    app::store::AppState,
    conversation::{model::ConversationState, workflows_types::WorkflowRunState},
};
use gpui::{AppContext, TestAppContext};
use serde_json::json;

fn setup(s: &mut AppState) -> String {
    let key = s.workspaces[0].key.clone();
    s.active_workspace = Some(key.clone());
    s.active = Some("parent".into());
    let mut conv = ConversationState::default();
    conv.workflow_runs.runs = vec![WorkflowRunState {
        run_id: "source".into(),
        tool_call_id: Some("source-tool".into()),
        status: "running".into(),
        ..Default::default()
    }];
    s.conversations.insert("parent".into(), conv);
    key
}
fn add_target(s: &mut AppState, tool: &str) {
    s.conversations
        .get_mut("parent")
        .unwrap()
        .workflow_runs
        .runs
        .push(WorkflowRunState {
            run_id: "target".into(),
            tool_call_id: Some(tool.into()),
            ..Default::default()
        });
}
#[gpui::test]
fn workflow_successor_requires_current_projected_identity(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, _| {
        let key = setup(s);
        s.conversations
            .get_mut("parent")
            .unwrap()
            .workflow_runs
            .runs[0]
            .superseded_by = Some("target".into());
        assert!(s.workflow_successor(&key, "parent", "source").is_none());
        add_target(s, "");
        assert!(s.workflow_successor(&key, "parent", "source").is_none());
        s.conversations
            .get_mut("parent")
            .unwrap()
            .workflow_runs
            .runs[1]
            .tool_call_id = Some("target-tool".into());
        let origin = s.workflow_link_origin(&key, "parent").unwrap();
        assert!(!s.open_workflow_successor(&origin, "source", "target", "wrong"));
        assert!(s.open_workflow_successor(&origin, "source", "target", "target-tool"));
        assert_eq!(s.workflow_focus().unwrap().run, "target");
        assert!(!s.open_workflow_successor(&origin, "source", "target", "target-tool"));
        s.all_workflow_runs();
        s.workspaces[0].generation += 1;
        assert!(!s.open_workflow_successor(&origin, "source", "target", "target-tool"));
        assert!(s.workflow_focus().is_none());
    });
}
#[gpui::test]
fn workflow_amend_follow_waits_for_exact_projection_in_both_orders(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        for stream_first in [false, true] {
            let key = setup(s);
            s.composer.update(cx, |c, _| c.set_text("parent draft"));
            assert!(s.focus_workflow(&key, "parent", "source", "source-tool"));
            s.begin_workflow_follow(&key, "parent", "source");
            if stream_first { add_target(s, "target-tool"); s.reconcile_workflow_follow(); }
            s.settle_workflow_follow(&key, "parent", "source", Some(&json!({"status":"accepted","result":{"type":"amendWorkflowRunSettings","runId":"target","toolCallId":"target-tool"}})));
            if !stream_first {
                assert_eq!(s.workflow_focus().unwrap().run, "source");
                add_target(s, "wrong-tool"); s.reconcile_workflow_follow();
                assert_eq!(s.workflow_focus().unwrap().run, "source");
                s.conversations.get_mut("parent").unwrap().workflow_runs.runs[1].tool_call_id = Some("target-tool".into());
                s.reconcile_workflow_follow();
            }
            assert_eq!(s.workflow_focus().unwrap().run, "target");
            assert!(s.workflow_follow.is_none());
            assert_eq!(s.composer.read(cx).text(), "parent draft");
        }
    });
}
#[gpui::test]
fn workflow_follow_discards_in_place_unknown_and_changed_owner(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, _| {
        for change in ["inplace", "unknown", "all", "workspace", "navigation", "connection"] {
            let key = setup(s); add_target(s, "target-tool");
            s.focus_workflow(&key, "parent", "source", "source-tool");
            s.begin_workflow_follow(&key, "parent", "source");
            match change {
                "all" => s.all_workflow_runs(), "workspace" => s.active_workspace = None,
                "navigation" => s.navigation_generation += 1, "connection" => s.workspaces[0].generation += 1,
                _ => {}
            }
            let target = if change == "inplace" { "source" } else { "target" };
            let ack = json!({"status":"accepted","result":{"type":"amendWorkflowRunSettings","runId":target,"toolCallId":"target-tool"}});
            s.settle_workflow_follow(&key, "parent", "source", if change == "unknown" { None } else { Some(&ack) });
            assert!(s.workflow_focus().is_none_or(|f| f.run != "target"));
            assert!(s.workflow_follow.is_none());
        }
    });
}
