use crate::{
    app::{
        dock::DockTab,
        root::RootView,
        store::AppState,
        test_support::{RUNTIME_TEST_LOCK, TestTargets},
    },
    files::{pane::FilesNode, preview::Preview},
};
use gpui::{Entity, MouseButton, TestApp, TestAppWindow, px, size};
use std::{path::Path, sync::Arc};

fn setup(root: &Path) -> (TestApp, Entity<AppState>, TestAppWindow<RootView>, String) {
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            root.join("prefs"),
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let key = app.update_entity(&state, |s, cx| {
        s.workspaces[0].path = root.to_path_buf();
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key.clone());
        s.composer.update(cx, |c, _| c.set_text("draft"));
        key
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(900.)));
    window.update(|v, window, cx| {
        window.bounds_changed(cx);
        v.dock_open = true;
        v.dock_tab = DockTab::Files;
        v.ensure_files_loaded(cx);
    });
    window.draw();
    (app, state, window, key)
}

#[test]
fn files_image_pointer_selection_and_stale_completion_preserve_chat_draft() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir(&root).unwrap();
    let path = root.join("image.png");
    let text = root.join("text.txt");
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([255, 0, 0, 255]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    std::fs::write(&path, bytes.into_inner()).unwrap();
    std::fs::write(&text, "current text").unwrap();
    let (app, _, mut window, key) = setup(&root);
    let row = app.read(|cx| cx.global::<TestTargets>().0["file-row-image.png"].center());
    window.simulate_click(row, MouseButton::Left);
    app.run_until_parked();
    let old_generation = window.read(|v, _| {
        assert_eq!(v.files.selected.as_ref(), Some(&path));
        let Some(Preview::Image(image)) = &v.files.preview else {
            panic!("background image read did not settle")
        };
        assert_eq!((image.width, image.height), (2, 1));
        v.files.generation
    });
    window.draw();
    window.update(|v, window, _| {
        let Some(Preview::Image(image)) = &v.files.preview else {
            panic!("image")
        };
        assert!(window.has_image_atlas_entry(&image.image));
    });
    let row = app.read(|cx| cx.global::<TestTargets>().0["file-row-text.txt"].center());
    window.simulate_click(row, MouseButton::Left);
    window.read(|v, _| assert_eq!(v.files.preview, Some(Preview::Text("current text".into()))));
    window.draw();
    let row = app.read(|cx| cx.global::<TestTargets>().0["file-row-image.png"].center());
    window.simulate_click(row, MouseButton::Left);
    window.update(|v, _, cx| {
        assert!(v.files.generation > old_generation);
        v.settle_file_preview(
            &key,
            &root,
            &path,
            old_generation,
            Preview::Text("stale".into()),
            cx,
        );
        assert!(matches!(v.files.preview, Some(Preview::Image(_))));
        assert_eq!(v.state.read(cx).composer.read(cx).text(), "draft");
    });
    window.draw();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn files_same_path_identity_and_workspace_reentry_reject_old_tree_and_preview() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir(&root).unwrap();
    let path = root.join("text.txt");
    std::fs::write(&path, "current").unwrap();
    let (_app, state, mut window, key) = setup(&root);
    window.update(|v, _, cx| {
        v.select_file(path.clone(), cx);
    });
    window.update(|v, _, cx| {
        let reset = v.files.reset_generation;
        let selection = v.files.generation;
        state.update(cx, |s, _| s.active_workspace = None);
        v.ensure_files_loaded(cx);
        assert!(v.files.root.is_none());
        assert!(v.files.roots.is_empty());
        assert!(v.files.preview.is_none());
        state.update(cx, |s, _| s.active_workspace = Some(key.clone()));
        v.ensure_files_loaded(cx);
        assert!(v.files.reset_generation > reset);
        v.select_file(path.clone(), cx);
        v.settle_file_preview(
            &key,
            &root,
            &path,
            selection,
            Preview::Text("old".into()),
            cx,
        );
        assert!(v.files.preview.is_none());
        v.settle_file_listing(&key, &root, reset, None, Vec::new(), cx);
        assert!(v.files.loading);
    });
    window.read(|v, _| {
        assert_eq!(v.files.preview, Some(Preview::Text("current".into())));
        assert_eq!(v.files.roots.len(), 1);
    });
    window.update(|v, _, cx| {
        let old_reset = v.files.reset_generation;
        let selection = v.files.generation;
        let replacement = "same-path-new-identity".to_string();
        state.update(cx, |s, _| {
            s.workspaces[0].key = replacement.clone();
            s.active_workspace = Some(replacement.clone());
        });
        v.ensure_files_loaded(cx);
        assert_eq!(v.files.workspace.as_deref(), Some(replacement.as_str()));
        assert!(v.files.roots.is_empty());
        assert!(v.files.selected.is_none());
        assert!(v.files.reset_generation > old_reset);
        let old_entries = vec![FilesNode {
            name: "old".into(),
            path: root.join("old"),
            is_dir: false,
            children: None,
        }];
        v.settle_file_listing(&key, &root, old_reset, None, old_entries, cx);
        v.settle_file_preview(
            &key,
            &root,
            &path,
            selection,
            Preview::Text("old".into()),
            cx,
        );
        assert!(v.files.roots.is_empty());
        assert!(v.files.preview.is_none());
    });
    window.read(|v, cx| {
        assert_eq!(v.files.roots.len(), 1);
        assert_eq!(v.state.read(cx).composer.read(cx).text(), "draft");
    });
    std::fs::remove_dir_all(root).unwrap();
}
