use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};
use gpui::TestApp;
use std::collections::HashSet;

#[test]
fn warm_targets_are_tasks_then_open_folders_without_the_active_one() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        let tmp = std::env::temp_dir();
        let mut tasks = WorkspaceHandle::new(tmp.join("warm-tasks"), vec![]);
        tasks.purpose = WorkspacePurpose::Conversation;
        let open = WorkspaceHandle::new(tmp.join("warm-open"), vec![]);
        let closed = WorkspaceHandle::new(tmp.join("warm-closed"), vec![]);
        let active = s.workspaces[0].key.clone();
        let (tasks_key, open_key) = (tasks.key.clone(), open.key.clone());
        s.workspaces.extend([open, closed, tasks]);
        s.active_workspace = Some(active.clone());
        s.sidebar_cache
            .set_open_folders(&HashSet::from([open_key.clone(), active]));
        assert_eq!(s.warm_targets(), [tasks_key, open_key]);
    });
}

#[test]
fn seeding_skips_workspaces_that_already_have_a_live_list() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::new();
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        let key = s.workspaces[0].key.clone();
        let row = |id: &str| crate::conversation::model::SessionEntry {
            session_id: id.into(),
            title: id.into(),
            phase: "idle".into(),
            last_activity_at: Some(1),
            sort_activity_at: Some(1),
            preview: String::new(),
            has_background_work: false,
        };
        s.sidebar_cache.record(&key, &[row("cached")]);
        s.workspaces[0].sessions = vec![row("live")];
        s.seed_cached_sessions();
        assert_eq!(s.workspaces[0].sessions[0].session_id, "live");
        assert!(!s.workspaces[0].sessions_cached);
    });
}
