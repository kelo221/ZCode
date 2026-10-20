//! Window test: the Review pane's read-only agent-branch comparison.

use crate::app::dock::DockTab;
use crate::app::root::RootView;
use crate::app::{
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::review::agent_branches::{AgentBranch, ReviewTarget, compare_status};
use gpui::{MouseButton, TestApp, TestAppWindow, px, size};
use std::path::PathBuf;
use std::sync::Arc;

fn agent_branch() -> AgentBranch {
    AgentBranch {
        repo_top: PathBuf::from("/repo"),
        worktree: PathBuf::from("/repo/.zcode/worktrees/agent_1"),
        branch: "zcode/agent_1".into(),
        base: "aaaaaaaaaa".into(),
        result: "bbbbbbbbbb".into(),
        files: 1,
        add: 3,
        del: 1,
    }
}

fn open_compare() -> (TestApp, TestAppWindow<RootView>) {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        s.active_workspace = Some(s.workspaces[0].key.clone());
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|v, window, cx| {
        window.bounds_changed(cx);
        v.dock_open = true;
        v.dock_tab = DockTab::Review;
        // The pane already asked about this workspace: drawing must not re-sync.
        v.git.requested = v.active_workspace_path(cx);
        let branch = agent_branch();
        v.git.agent_branches = vec![branch.clone()];
        v.git.set_target(ReviewTarget::AgentBranch(branch.clone()));
        let status = compare_status(&branch, "M\0src/lib.rs\0", "3\t1\tsrc/lib.rs\0");
        v.git.apply_status(branch.repo_top.clone(), status);
    });
    window.draw();
    window.draw();
    (app, window)
}

fn targets(app: &TestApp) -> std::collections::HashMap<String, gpui::Bounds<gpui::Pixels>> {
    app.read(|cx| cx.global::<TestTargets>().0.clone())
}

#[test]
fn agent_branch_comparison_is_read_only_and_returns_to_the_working_tree() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, mut window) = open_compare();
    let t = targets(&app);
    assert!(t.contains_key("agent-branch-zcode/agent_1"));
    assert!(
        t.contains_key("compare-close"),
        "compare mode offers the way back"
    );
    assert!(!t.contains_key("stage-all"), "no staging in a comparison");
    assert!(
        !t.contains_key("review-commit-bar"),
        "no commit or push in a comparison"
    );

    // Mutating actions are refused even if reached some other way.
    window.update(|v, _, cx| {
        v.stage_all(cx);
        assert!(!v.git.busy);
    });

    window.simulate_click(t["compare-close"].center(), MouseButton::Left);
    window.read(|v, _| assert_eq!(v.git.target, ReviewTarget::WorkingTree));
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    let t = targets(&app);
    assert!(t.contains_key("stage-all"));
    assert!(t.contains_key("review-commit-bar"));
}

#[test]
fn clicking_an_agent_branch_pins_the_comparison() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut app, mut window) = open_compare();
    window.update(|v, _, _| {
        v.git.set_target(ReviewTarget::WorkingTree);
    });
    app.update(|cx| cx.global_mut::<TestTargets>().0.clear());
    window.draw();
    let t = targets(&app);
    window.simulate_click(t["agent-branch-zcode/agent_1"].center(), MouseButton::Left);
    window.read(|v, _| {
        assert_eq!(v.git.target, ReviewTarget::AgentBranch(agent_branch()));
    });
}
