use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use gpui::{ClipboardItem, Image, ImageFormat, TestApp, px, size};
use std::sync::Arc;

#[test]
fn new_draft_clipboard_preserves_format_and_explicit_temp_ownership_without_upload() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    app.update_entity(&state, |s, _| {
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        s.active_workspace = Some(s.workspaces[0].key.clone());
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| {
        let focus = state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.bounds_changed(cx);
    });
    app.update(|cx| {
        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
            ImageFormat::Jpeg,
            vec![1, 2, 3],
        )))
    });
    window.draw();
    window.simulate_keystroke("ctrl-v");
    window.draw();
    let path = app.update_entity(&state, |s, cx| {
        assert!(s.workspaces[0].image_uploads.is_empty());
        assert!(s.active.is_none());
        let composer = s.composer.read(cx);
        assert_eq!(composer.attachments[0].mime, "image/jpeg");
        let path = std::path::PathBuf::from(&composer.attachments[0].reference);
        assert_eq!(path.extension().unwrap(), "jpg");
        assert_eq!(composer.temp_owned, vec![path.clone()]);
        path
    });
    assert!(rx.try_recv().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3]);
    crate::shared::temp_attachments::delete_owned(&path);
}
