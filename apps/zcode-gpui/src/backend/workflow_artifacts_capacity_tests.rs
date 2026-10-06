use crate::{app::store::AppState, conversation::workflows_types::WorkflowRunState};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn artifact_query_cache_and_pending_reads_are_bounded_and_background_results_cannot_settle(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        let runs = (0..40)
            .map(|i| WorkflowRunState {
                run_id: format!("run-{i}"),
                tool_call_id: Some(format!("tool-{i}")),
                status: "completed".into(),
                ..Default::default()
            })
            .collect();
        s.conversations
            .entry("parent".into())
            .or_default()
            .workflow_runs
            .runs = runs;
        for i in 0..40 {
            assert!(s.focus_workflow(&key, "parent", &format!("run-{i}"), &format!("tool-{i}")));
            s.ensure_workflow_artifacts(false, cx);
        }
        assert_eq!(s.workspaces[0].workflow_artifacts.len(), 32);
        assert_eq!(s.workspaces[0].pending.len(), 32);
        let reads = rx
            .try_iter()
            .map(|line| serde_json::from_str::<Value>(&line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), 40);
        for read in reads {
            s.handle_response(
                &key,
                read["id"].as_u64().unwrap(),
                Some(json!({"artifacts":[]})),
                None,
                cx,
            );
        }
        assert!(s.workspaces[0].pending.is_empty());
        assert_eq!(
            s.workspaces[0]
                .workflow_artifacts
                .values()
                .filter(|q| q.query.value.is_some())
                .count(),
            1
        );
        assert!(
            s.active_workflow_artifacts()
                .unwrap()
                .query
                .value
                .as_ref()
                .unwrap()
                .is_empty()
        );
    });
}
