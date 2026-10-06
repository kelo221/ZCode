use super::*;
use crate::sessions::items::{
    SESSION_EXTENDED_ROWS, SESSION_PREVIEW_ROWS, load_more_label, next_limit_step,
    visible_row_indices, visible_session_count,
};
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
fn session_limits_progress_preview_extended_all() {
    // Initial: the 3 latest threads only.
    assert_eq!(visible_session_count(0, 100), SESSION_PREVIEW_ROWS);
    assert_eq!(visible_session_count(0, 2), 2, "no padding of short lists");
    // First "Load more" click: the extended window.
    assert_eq!(visible_session_count(1, 100), SESSION_EXTENDED_ROWS);
    assert_eq!(visible_session_count(1, 10), 10);
    // Second click: everything.
    assert_eq!(visible_session_count(2, 100), 100);
    assert_eq!(visible_session_count(3, 100), 100, "clamped at all");
}

#[test]
fn old_active_row_is_pinned_without_stretching_the_cap() {
    // Review 12.1: active index 900 must not render 901 rows.
    let (visible, pinned) = visible_row_indices(0, 1000, Some(900));
    assert_eq!(visible, SESSION_PREVIEW_ROWS);
    assert_eq!(pinned, Some(900));
    let (visible, pinned) = visible_row_indices(0, 1000, Some(1));
    assert_eq!(visible, SESSION_PREVIEW_ROWS);
    assert_eq!(pinned, None, "in-window active is not double-rendered");
}

#[test]
fn load_more_escalates_to_all_on_the_second_click() {
    assert_eq!(next_limit_step(0), 1);
    assert_eq!(next_limit_step(1), 2);
    assert_eq!(next_limit_step(2), 2, "no third step");
    assert_eq!(load_more_label(0, 97), "Load more (97 more)");
    assert_eq!(load_more_label(1, 75), "Load all (75 more)");
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
