use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose, canonical_workspace_string};
use crate::conversation::model::{ConversationState, SessionEntry};
use crate::shared::preferences::Preferences;
use gpui::{Entity, TestApp};

fn entry(sid: &str, has_background_work: bool) -> SessionEntry {
    SessionEntry {
        session_id: sid.into(),
        title: sid.into(),
        phase: "completedSuccess".into(),
        last_activity_at: None,
        preview: String::new(),
        has_background_work,
    }
}

fn test_app() -> TestApp {
    TestApp::with_text_system_and_assets(
        std::sync::Arc::new(gpui::NoopTextSystem::new()),
        std::sync::Arc::new(ely_gpui_component::Assets),
    )
}

fn local(path: &str) -> serde_json::Value {
    serde_json::json!({"kind": "local", "workspacePath": path, "workspacePurpose": "project"})
}

/// Tasks workspace plus projects `a` and `b`, both persisted in `setting.json`;
/// `a` is active and owns session `s1`.
fn setup(app: &mut TestApp, root: &std::path::Path) -> (Entity<AppState>, String, String) {
    let settings = root.join("setting.json");
    let (a_key, b_key) = (
        canonical_workspace_string(&root.join("a")),
        canonical_workspace_string(&root.join("b")),
    );
    let seeded = serde_json::json!({
        "recentProjects": [a_key, b_key],
        "lastWorkspaceSession": [local(&a_key), local(&b_key)],
    });
    std::fs::write(&settings, seeded.to_string()).unwrap();
    app.update(|cx| {
        ely_gpui_component::init(cx);
        Preferences::install_at(
            Default::default(),
            settings,
            std::sync::Arc::new(|| false),
            cx,
        )
    });
    let state = app.new_entity(AppState::for_test);
    let keys = app.update_entity(&state, |s, _| {
        let mut conv = WorkspaceHandle::new(root.join("tasks"), vec![]);
        conv.purpose = WorkspacePurpose::Conversation;
        let mut a = WorkspaceHandle::new(root.join("a"), vec![]);
        a.purpose = WorkspacePurpose::Project;
        a.sessions = vec![entry("s1", false)];
        let mut b = WorkspaceHandle::new(root.join("b"), vec![]);
        b.purpose = WorkspacePurpose::Project;
        let keys = (a.key.clone(), b.key.clone());
        s.workspaces = vec![conv, a, b];
        s.active_workspace = Some(keys.0.clone());
        s.conversations
            .insert("s1".into(), ConversationState::default());
        s.session_drafts.insert("s1".into(), "draft".into());
        keys
    });
    (state, keys.0, keys.1)
}

fn temp_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("gpui-remove-ws-{}", uuid::Uuid::now_v7()));
    for d in ["tasks", "a", "b"] {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    root
}

#[test]
fn removing_the_active_project_moves_to_the_next_and_drops_its_mirrors() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp_root();
    let mut app = test_app();
    let (state, a, b) = setup(&mut app, &root);
    let removed = app.update_entity(&state, |s, cx| s.remove_workspace(&a, cx));
    assert!(removed);
    app.update_entity(&state, |s, _| {
        assert!(s.ws(&a).is_none());
        assert_eq!(s.active_workspace.as_deref(), Some(b.as_str()));
        assert!(!s.conversations.contains_key("s1"));
        assert!(!s.session_drafts.contains_key("s1"));
    });
    for _ in 0..1000 {
        app.run_until_parked();
        if app.read(Preferences::drained) {
            break;
        }
        std::thread::yield_now();
    }
    let disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("setting.json")).unwrap()).unwrap();
    assert_eq!(disk["recentProjects"], serde_json::json!([b]));
    let session = disk["lastWorkspaceSession"].as_array().unwrap();
    assert_eq!(session.len(), 1);
    assert_eq!(session[0]["workspacePath"], b.as_str());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_project_with_live_work_is_not_removed() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp_root();
    let mut app = test_app();
    let (state, a, _) = setup(&mut app, &root);
    let removed = app.update_entity(&state, |s, cx| {
        s.ws_mut(&a).unwrap().sessions = vec![entry("s1", true)];
        s.remove_workspace(&a, cx)
    });
    assert!(!removed);
    app.update_entity(&state, |s, _| {
        assert!(s.ws(&a).is_some());
        assert!(s.conversations.contains_key("s1"));
        assert!(s.errors.back().unwrap().contains("running tasks"));
    });
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_tasks_workspace_cannot_be_removed() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = temp_root();
    let mut app = test_app();
    let (state, _, _) = setup(&mut app, &root);
    app.update_entity(&state, |s, cx| {
        let tasks = s.conversation_workspace_key().unwrap();
        assert!(!s.remove_workspace(&tasks, cx));
        assert!(s.ws(&tasks).is_some());
    });
    let _ = std::fs::remove_dir_all(root);
}
