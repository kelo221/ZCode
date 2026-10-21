use crate::app::{root::RootView, store::AppState, test_support::*};
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};
use crate::shared::{preferences::Preferences, settings::AppSettings};
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

/// More projects than the old 8-entry cap all render as rows, and a right
/// click opens the row's menu instead of toggling or activating the folder.
#[test]
fn every_project_renders_and_right_click_leaves_the_folder_alone() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("gpui-sidebar-rows-{}", uuid::Uuid::now_v7()));
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        Preferences::install_at(
            AppSettings::default(),
            root.join("setting.json"),
            Arc::new(|| false),
            cx,
        );
    });
    let names: Vec<String> = (0..12).map(|i| format!("proj{i:02}")).collect();
    let state = app.new_entity(AppState::for_test);
    let keys: Vec<String> = app.update_entity(&state, |s, _| {
        s.workspaces = names
            .iter()
            .map(|n| {
                let mut w = WorkspaceHandle::new(root.join(n), vec![]);
                w.purpose = WorkspacePurpose::Project;
                w.display = n.clone();
                w
            })
            .collect();
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.workspaces.iter().map(|w| w.key.clone()).collect()
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(2000.)));
    window.draw();
    let last = app.read(|cx| {
        let targets = &cx.global::<TestTargets>().0;
        for n in &names {
            assert!(
                targets.contains_key(&format!("project-row-{n}")),
                "{n} renders"
            );
        }
        targets["project-row-proj11"].center()
    });
    window.simulate_click(last, MouseButton::Right);
    window.draw();
    window.read(|view, cx| {
        assert!(!view.folders.opened.contains(&keys[11]));
        assert!(!view.folders.collapsed.contains(&keys[11]));
        assert_eq!(
            view.state.read(cx).active_workspace.as_deref(),
            Some(keys[0].as_str())
        );
    });
}
