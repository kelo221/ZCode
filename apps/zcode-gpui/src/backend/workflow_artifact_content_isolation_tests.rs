use crate::{app::store::AppState, conversation::workflows_types::WorkflowRunState};
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};

#[gpui::test]
fn markdown_selection_tokens_parent_changes_and_failed_enqueue_do_not_adopt_or_leak(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx); s.workspaces[0].started = true;
        s.active_workspace = Some(key.clone()); s.active = Some("parent".into());
        s.conversations.entry("parent".into()).or_default().workflow_runs.runs.push(WorkflowRunState {
            run_id: "run".into(), tool_call_id: Some("tool".into()), status: "completed".into(), ..Default::default()
        });
        assert!(s.focus_workflow(&key, "parent", "run", "tool"));
        s.ensure_workflow_artifacts(false, cx);
        let req: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, req["id"].as_u64().unwrap(), Some(json!({"artifacts":[{
            "id":"report","kind":"markdown","contentType":"text/markdown","version":1,
            "versions":[{"version":1,"publishedAt":1}],"itemCount":0
        }]})), None, cx);
        let origin = s.workflow_focus().unwrap().clone();
        s.view_workflow_markdown(&origin, "report", 1, cx);
        let old: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        let selection = s.active_workflow_artifacts().unwrap().content.as_ref().unwrap().selection.clone();
        s.close_workflow_markdown(&origin, "report", 1, cx);
        s.view_workflow_markdown(&origin, "report", 1, cx);
        let new: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert!(!s.markdown_selection_current(&origin, &selection));
        s.handle_response(&key, old["id"].as_u64().unwrap(), Some(json!({"dataBase64":"","mediaType":"text/markdown","totalBytes":0,"nextOffset":null})), None, cx);
        assert!(s.active_workflow_artifacts().unwrap().content.as_ref().unwrap().query.value.is_none());
        s.active = Some("other-parent".into());
        s.handle_response(&key, new["id"].as_u64().unwrap(), Some(json!({"dataBase64":"","mediaType":"text/markdown","totalBytes":0,"nextOffset":null})), None, cx);
        assert!(s.active_workflow_artifacts().is_none());
        s.retire_stale_workflow_artifact_contents();
        assert!(s.workspaces[0].workflow_artifacts.values().all(|q| q.content.is_none()));
        s.active = Some("parent".into());
        s.all_workflow_runs();
        assert!(s.workspaces[0].workflow_artifacts.values().all(|q| q.content.is_none()));
        assert!(s.workspaces[0].pending.is_empty());
        assert!(s.focus_workflow(&key, "parent", "run", "tool"));
        s.ensure_workflow_artifacts(false, cx);
        let req: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.handle_response(&key, req["id"].as_u64().unwrap(), Some(json!({"artifacts":[{
            "id":"report","kind":"markdown","contentType":"text/markdown","version":1,
            "versions":[{"version":1,"publishedAt":1}],"itemCount":0
        }]})), None, cx);
        let origin = s.workflow_focus().unwrap().clone();
        drop(rx);
        s.view_workflow_markdown(&origin, "report", 1, cx);
        let content = s.active_workflow_artifacts().unwrap().content.as_ref().unwrap();
        assert!(!content.query.loading); assert!(content.query.error.is_some());
        assert!(s.workspaces[0].pending.is_empty());
    });
}
