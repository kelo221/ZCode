use crate::{
    app::{root::RootView, store::AppState, test_support::RUNTIME_TEST_LOCK},
    terminal::pane::{CELL_H, CELL_W, PtyBridgeListener, TermDims},
};
use gpui::{MouseButton, TestApp, point, px, size};
use std::sync::{Arc, Mutex};

struct RecordingWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn terminal_drag_copy_and_no_selection_interrupt_preserve_chat_and_spawn_isolation() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let key = app.update_entity(&state, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.active_workspace = Some(key.clone());
        s.composer.update(cx, |c, _| c.set_text("draft"));
        key
    });
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(900.)));
    window.update(|v, window, cx| {
        window.bounds_changed(cx);
        v.term_open = true;
        v.term.workspace = Some(key.clone());
        v.term.dims = TermDims { cols: 20, rows: 3 };
        v.term.test_cell_metrics = Some(Default::default());
        *v.term.writer.lock().unwrap() = Some(Box::new(RecordingWriter(output.clone())));
        let listener = PtyBridgeListener {
            writer: v.term.writer.clone(),
        };
        let mut term =
            alacritty_terminal::term::Term::new(Default::default(), &v.term.dims, listener);
        v.term
            .processor
            .advance(&mut term, b"hello world\r\nnext line");
        v.term.term = Some(term);
    });
    window.draw();
    let bounds = window.read(|v, _| v.term.grid_bounds.lock().unwrap().unwrap());
    assert!(bounds.size.width > px(0.));
    let start = bounds.origin + point(px(CELL_W * 0.1), px(CELL_H * 0.5));
    let end = bounds.origin + point(px(CELL_W * 4.9), px(CELL_H * 0.5));
    window.simulate_mouse_down(start, MouseButton::Left);
    window.simulate_mouse_move(end);
    window.simulate_mouse_up(end, MouseButton::Left);
    window.read(|v, _| {
        assert!(!v.term.selecting);
        assert_eq!(
            v.term
                .term
                .as_ref()
                .unwrap()
                .selection_to_string()
                .as_deref(),
            Some("hello")
        );
    });
    window.draw();
    window.update(|_, window, _| {
        let background: gpui::Background = gpui::rgb(crate::terminal::grid::SELECTION_BG).into();
        assert!(
            window
                .painted_quads()
                .iter()
                .any(|quad| quad.background == background
                    && quad.bounds.size.width > gpui::ScaledPixels::default()
                    && quad.bounds.size.height > gpui::ScaledPixels::default())
        );
    });
    window.simulate_keystroke("ctrl-c");
    assert_eq!(
        app.read_from_clipboard().and_then(|item| item.text()),
        Some("hello".into())
    );
    assert!(output.lock().unwrap().is_empty());
    window.simulate_keystroke("cmd-c");
    assert_eq!(
        app.read_from_clipboard().and_then(|item| item.text()),
        Some("hello".into())
    );
    assert!(output.lock().unwrap().is_empty());
    window.draw();
    window.simulate_click(start, MouseButton::Left);
    window.simulate_keystroke("ctrl-c");
    assert_eq!(*output.lock().unwrap(), vec![3]);
    window.simulate_mouse_down(start, MouseButton::Left);
    window.simulate_mouse_up(point(px(0.), px(0.)), MouseButton::Left);
    window.read(|v, _| assert!(!v.term.selecting));
    window.update(|v, _, cx| {
        let owner = v.terminal_owner();
        let old_writer = v.term.writer.clone();
        v.term.shutdown();
        assert!(v.term.grid_bounds.lock().unwrap().is_none());
        assert!(!v.term.selecting);
        v.term.workspace = Some(key.clone());
        v.term.writer = Arc::new(Mutex::new(None));
        assert!(!v.terminal_owner_current(&owner, cx));
        assert!(!Arc::ptr_eq(&old_writer, &v.term.writer));
        state.update(cx, |s, _| s.active_workspace = None);
        v.term_key(&gpui::Keystroke::parse("ctrl-c").unwrap(), cx);
        assert_eq!(*output.lock().unwrap(), vec![3]);
        assert_eq!(v.state.read(cx).composer.read(cx).text(), "draft");
    });
}
