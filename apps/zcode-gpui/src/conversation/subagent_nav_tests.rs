use super::*;
use std::collections::HashMap;

#[test]
fn read_only_view_predicate() {
    assert!(!is_read_only(None));
    assert!(is_read_only(Some("child-1")));
}

#[test]
fn parent_session_resolution() {
    let mut child_owner = HashMap::new();
    child_owner.insert(
        "child-1".to_string(),
        ("ws-1".to_string(), "parent-1".to_string()),
    );

    assert_eq!(parent_session_id(&child_owner, None), None);
    assert_eq!(
        parent_session_id(&child_owner, Some("child-1")),
        Some("parent-1")
    );
    assert_eq!(parent_session_id(&child_owner, Some("child-unknown")), None);
}

#[test]
fn child_topic_workspace_routing() {
    let mut child_owner = HashMap::new();
    child_owner.insert(
        "child-42".to_string(),
        ("ws-xyz".to_string(), "parent-1".to_string()),
    );

    assert_eq!(
        child_topic_workspace(&child_owner, "conversation/child-42"),
        Some("ws-xyz".to_string())
    );
    assert_eq!(
        child_topic_workspace(&child_owner, "conversation/unknown-child"),
        None
    );
    assert_eq!(child_topic_workspace(&child_owner, "status/child-42"), None);
}

#[test]
fn active_conversation_sid_fallback() {
    assert_eq!(active_conversation_sid(None, None), None);
    assert_eq!(
        active_conversation_sid(None, Some("parent-1")),
        Some("parent-1")
    );
    assert_eq!(
        active_conversation_sid(Some("child-1"), Some("parent-1")),
        Some("child-1")
    );
    assert_eq!(
        active_conversation_sid(Some("child-1"), None),
        Some("child-1")
    );
}

#[test]
fn child_session_id_collection() {
    let mut child_owner = HashMap::new();
    child_owner.insert(
        "child-owner-1".to_string(),
        ("ws".to_string(), "p".to_string()),
    );

    let sub_state = crate::conversation::subagents::SubagentsState {
        child_session_ids: vec!["subagent-child-2".to_string(), "shared-child".to_string()],
        ..Default::default()
    };
    child_owner.insert(
        "shared-child".to_string(),
        ("ws".to_string(), "p".to_string()),
    );

    let collected = collect_child_session_ids(&child_owner, [&sub_state]);
    assert_eq!(collected.len(), 3);
    assert!(collected.contains("child-owner-1"));
    assert!(collected.contains("subagent-child-2"));
    assert!(collected.contains("shared-child"));
}
