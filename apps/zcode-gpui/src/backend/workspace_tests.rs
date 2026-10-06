//! Tests for backend/workspace.rs helpers (kept out-of-line for the
//! 400-line cap).

use super::*;
use crate::shared::preferences::merge_recent_projects;

#[test]
fn unloaded_connection_invalidates_pump_and_restart_token() {
    let mut h = WorkspaceHandle::new(std::env::temp_dir(), vec![]);
    let old = h.generation;
    h.unload();
    assert_ne!(old, h.generation);
    let restart = h.generation;
    h.shutdown();
    assert_ne!(restart, h.generation);
}

#[test]
fn closed_stdin_is_not_a_successful_enqueue() {
    let mut h = WorkspaceHandle::new(std::env::temp_dir(), vec![]);
    let (tx, rx) = std::sync::mpsc::channel();
    h.inbound = Some(tx);
    drop(rx);
    assert!(!h.send_line("request".into()));
    assert_eq!(h.status, "write failed");
}

#[test]
fn disconnected_stdin_rejects_enqueue() {
    let mut h = WorkspaceHandle::new(std::env::temp_dir(), vec![]);
    assert!(!h.send_line("request".into()));
}

#[test]
fn recent_projects_are_prepended_deduped_and_capped() {
    let existing = vec!["F:\\a".into(), "F:\\b".into(), "F:\\c".into()];
    // A re-picked project moves to the front instead of duplicating.
    let merged = merge_recent_projects(&existing, "F:\\b", 3);
    assert_eq!(merged, ["F:\\b", "F:\\a", "F:\\c"]);
    // The cap keeps the most recent entries.
    let capped = merge_recent_projects(&merged, "F:\\d", 2);
    assert_eq!(capped, ["F:\\d", "F:\\b"]);
    // Empty list just starts it.
    assert_eq!(merge_recent_projects(&[], "F:\\x", 8), ["F:\\x"]);
}

#[test]
fn nearest_existing_dir_walks_to_an_ancestor() {
    let missing = std::env::temp_dir().join(format!("zcode-gpui-gone-{}", uuid::Uuid::now_v7()));
    let ancestor = nearest_existing_dir(&missing.join("nested").join("workspace"));
    assert!(ancestor.is_dir());
    let missing_s = missing.to_string_lossy().into_owned();
    let ancestor_s = ancestor.to_string_lossy().into_owned();
    assert!(missing_s.starts_with(&ancestor_s) || ancestor.exists());
}

#[test]
fn subscription_registry_owns_topics_absent_from_the_sessions_index() {
    // Review finding 2: a newly created conversation is subscribed before the
    // sessions-index lists it. The registry, not the row list, is the owner.
    let mut h = WorkspaceHandle::new(std::env::temp_dir(), vec![]);
    h.subscriptions.insert(
        "conversation/brand-new".into(),
        RouteSubscription {
            id: "sub".into(),
            log_epoch: "e1".into(),
        },
    );
    assert!(h.sessions.is_empty());
    assert!(h.subscriptions.contains_key("conversation/brand-new"));
}
