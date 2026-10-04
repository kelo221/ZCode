use super::*;
use std::collections::HashSet;

fn dummy_session(id: &str, title: &str, preview: &str) -> crate::conversation::model::SessionEntry {
    crate::conversation::model::SessionEntry {
        session_id: id.to_string(),
        title: title.to_string(),
        phase: "idle".to_string(),
        last_activity_at: None,
        preview: preview.to_string(),
    }
}

#[test]
fn filter_session_indices_omits_subagents() {
    let sessions = vec![
        dummy_session("parent-1", "Main Task", "hello"),
        dummy_session("subagent-child-1", "Subagent Work", "details"),
        dummy_session("parent-2", "Another Feature", "world"),
        dummy_session("subagent-child-2", "Explore Code", "grep"),
    ];

    let mut child_sids = HashSet::new();
    child_sids.insert("subagent-child-1");
    child_sids.insert("subagent-child-2");

    let indices = filter_session_indices(&sessions, &child_sids, "");
    assert_eq!(indices, vec![0, 2]);
}

#[test]
fn filter_session_indices_with_search_and_subagents() {
    let sessions = vec![
        dummy_session("s1", "Refactor Auth", "jwt"),
        dummy_session("sub-1", "Auth Subagent", "tokens"),
        dummy_session("s2", "Fix Bug", "auth issue"),
        dummy_session("s3", "Unrelated", "misc"),
    ];

    let mut child_sids = HashSet::new();
    child_sids.insert("sub-1");

    // Search matches s1 and s2, but sub-1 is excluded because it's a child subagent.
    let indices = filter_session_indices(&sessions, &child_sids, "auth");
    assert_eq!(indices, vec![0, 2]);
}

#[test]
fn test_conversation_workspace_dir_resolution() {
    let conv_dir = crate::backend::workspace::conversation_workspace_dir();
    assert!(
        conv_dir.ends_with(
            std::path::Path::new(".zcode")
                .join("workspace")
                .join("default")
        )
    );
    assert_eq!(
        crate::backend::workspace::WorkspacePurpose::Conversation,
        crate::backend::workspace::WorkspacePurpose::Conversation
    );
}
