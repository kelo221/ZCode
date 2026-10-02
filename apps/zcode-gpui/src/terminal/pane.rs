//! Terminal state & PTY process management: one local PTY per workspace
//! (portable-pty/ConPTY) parsed by alacritty_terminal into grid rows.
//! Output arrives on a reader thread into a shared buffer; a 60ms poll drains it
//! on the main thread.

use crate::terminal::grid;
use crate::terminal::io::{dims_for_pixels, shell_command, spawn_reader};
use crate::app::root::RootView;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use gpui::{actions, AppContext, Context, FocusHandle, Keystroke, Pixels, Point, Task};
use portable_pty::{native_pty_system, MasterPty, PtySize};

// Ctrl+` toggles the bottom terminal drawer (aligned with desktop shortcut).
actions!(zcode_gpui, [ToggleTerminal]);
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub const CELL_W: f32 = 6.6;
pub const CELL_H: f32 = 15.0;

/// Terminal dimensions implementing alacritty's `Dimensions`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

impl alacritty_terminal::grid::Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Forwards terminal response events (e.g. DSR cursor reports) to the PTY.
pub struct PtyBridgeListener {
    pub(crate) writer: SharedWriter,
}

impl EventListener for PtyBridgeListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event
            && let Ok(mut lock) = self.writer.lock()
            && let Some(w) = lock.as_mut()
        {
            let _ = std::io::Write::write_all(w, text.as_bytes());
            let _ = std::io::Write::flush(w);
        }
    }
}

pub type SharedWriter = Arc<Mutex<Option<Box<dyn std::io::Write + Send>>>>;

pub struct TermPane {
    pub workspace: Option<String>,
    pub term: Option<Term<PtyBridgeListener>>,
    pub processor: Processor,
    pub writer: SharedWriter,
    pub master: Option<Box<dyn MasterPty + Send>>,
    pub child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    /// Output channel of the current spawn. Replaced (not reused) per spawn so
    /// a previous shell's reader thread can never feed the new grid.
    pub buffer: Arc<Mutex<Vec<u8>>>,
    pub dirty: Arc<AtomicBool>,
    /// The shell process has exited; the next open respawns it.
    pub exited: bool,
    /// Poll loop for the current spawn; dropping it cancels the loop.
    pub poll_task: Option<Task<()>>,
    /// Pixel size measured by the paint-phase probe, applied on the poll tick.
    pub pending_size: Arc<Mutex<Option<(f32, f32)>>>,
    pub focus: FocusHandle,
    pub dims: TermDims,
    pub middle_anchor: Option<Point<Pixels>>,
}

impl TermPane {
    /// The focus handle belongs to RootView (TermPane lives inside it).
    pub fn new(focus: FocusHandle) -> Self {
        Self {
            workspace: None,
            term: None,
            processor: Processor::default(),
            writer: Arc::new(Mutex::new(None)),
            master: None,
            child: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            dirty: Arc::new(AtomicBool::new(false)),
            exited: false,
            poll_task: None,
            pending_size: Arc::new(Mutex::new(None)),
            focus,
            dims: TermDims { cols: 80, rows: 24 },
            middle_anchor: None,
        }
    }

    /// Tear down the current shell: kill and reap the child, close the PTY
    /// (unblocking the reader thread) and cancel the poll loop.
    pub fn shutdown(&mut self) {
        self.poll_task = None;
        if let Ok(mut w) = self.writer.lock() {
            *w = None;
        }
        self.master = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            // Reap off-thread so a slow exit never blocks the UI thread.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        self.term = None;
        self.workspace = None;
        self.exited = false;
    }

    /// Detach a shell that exited on its own, keeping its last screen visible.
    fn mark_exited(&mut self) {
        self.exited = true;
        self.child = None;
        self.master = None;
        if let Ok(mut w) = self.writer.lock() {
            *w = None;
        }
    }
}

impl Drop for TermPane {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl RootView {
    /// Spawn the PTY (once per workspace) and start the poll loop.
    pub(crate) fn ensure_term(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.state.read(cx).active_ws_key() else {
            return;
        };
        if self.term.workspace.as_deref() == Some(ws.as_str()) && !self.term.exited {
            return;
        }
        let Some(path) = self.active_workspace_path(cx) else {
            return;
        };
        // Replace (never share) per-spawn channels: the previous shell's
        // reader thread may still be draining and must not reach this grid.
        self.term.shutdown();
        self.term.writer = Arc::new(Mutex::new(None));
        self.term.buffer = Arc::new(Mutex::new(Vec::new()));
        self.term.dirty = Arc::new(AtomicBool::new(false));
        self.term.processor = Processor::default();
        let listener = PtyBridgeListener {
            writer: self.term.writer.clone(),
        };
        let term = Term::new(Config::default(), &self.term.dims, listener);
        let pty = match native_pty_system().openpty(PtySize {
            rows: self.term.dims.rows as u16,
            cols: self.term.dims.cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(e) => {
                self.state
                    .update(cx, |s, _| s.push_log(format!("pty open failed: {e}")));
                return;
            }
        };
        let cmd = shell_command(&path);
        let child = match pty.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(e) => {
                self.state
                    .update(cx, |s, _| s.push_log(format!("shell spawn failed: {e}")));
                return;
            }
        };
        // The slave end belongs to the child now; holding it would keep the
        // PTY open after the shell exits.
        drop(pty.slave);
        let writer = pty.master.take_writer().ok();
        let reader = pty.master.try_clone_reader().ok();
        if let Ok(mut w) = self.term.writer.lock() {
            *w = writer;
        }
        self.term.workspace = Some(ws);
        self.term.term = Some(term);
        self.term.master = Some(pty.master);
        self.term.child = Some(child);
        if let Some(reader) = reader {
            spawn_reader(
                reader,
                self.term.buffer.clone(),
                self.term.dirty.clone(),
                self.term.writer.clone(),
            );
        }
        self.start_term_poll(cx);
        cx.notify();
    }

    fn start_term_poll(&mut self, cx: &mut Context<Self>) {
        let buffer = self.term.buffer.clone();
        let dirty = self.term.dirty.clone();
        let pending = self.term.pending_size.clone();
        // Held in `TermPane::poll_task`: a respawn or view drop cancels it.
        self.term.poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_spawn(async {
                    std::thread::sleep(std::time::Duration::from_millis(60));
                })
                .await;
                let bytes = if dirty.swap(false, Ordering::Relaxed) {
                    buffer.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default()
                } else {
                    Vec::new()
                };
                let size = pending.lock().ok().and_then(|mut p| p.take());
                let alive = this
                    .update(cx, |v, cx| {
                        if !bytes.is_empty()
                            && let Some(term) = v.term.term.as_mut()
                        {
                            v.term.processor.advance(term, &bytes);
                        }
                        if let Some((w, h)) = size {
                            v.resize_term(w, h);
                        }
                        let exited = v
                            .term
                            .child
                            .as_mut()
                            .is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
                        if exited {
                            // Flush output that raced the exit before stopping.
                            let rest = buffer
                                .lock()
                                .map(|mut b| std::mem::take(&mut *b))
                                .unwrap_or_default();
                            if !rest.is_empty()
                                && let Some(term) = v.term.term.as_mut()
                            {
                                v.term.processor.advance(term, &rest);
                            }
                            v.term.mark_exited();
                        }
                        if !bytes.is_empty() || size.is_some() || exited {
                            cx.notify();
                        }
                        !exited
                    })
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        }));
    }

    /// Apply a measured pixel size to the PTY and grid.
    pub(crate) fn resize_term(&mut self, width: f32, height: f32) {
        let dims = dims_for_pixels(width, height);
        let (cols, rows) = (dims.cols, dims.rows);
        if self.term.dims == dims {
            return;
        }
        self.term.dims = dims;
        if let Some(term) = self.term.term.as_mut() {
            term.resize(self.term.dims);
        }
        if let Some(master) = self.term.master.as_ref() {
            let _ = master.resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub(crate) fn term_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let k = keystroke.key.as_str();
        if k == "v" && (keystroke.modifiers.control || keystroke.modifiers.platform) {
            if let Some(item) = cx.read_from_clipboard()
                && let Some(text) = item.text()
            {
                self.write_term(text.as_bytes());
            }
            cx.stop_propagation();
            return;
        }
        let bytes = grid::key_to_bytes(
            k,
            keystroke.modifiers.control,
            keystroke.modifiers.alt,
            keystroke.modifiers.shift,
        );
        if !bytes.is_empty() {
            self.write_term(&bytes);
            cx.stop_propagation();
        }
    }

    pub(crate) fn write_term(&mut self, bytes: &[u8]) {
        if let Ok(mut lock) = self.term.writer.lock()
            && let Some(w) = lock.as_mut()
        {
            let _ = std::io::Write::write_all(w, bytes);
            let _ = std::io::Write::flush(w);
        }
    }
}
