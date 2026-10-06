//! PTY I/O helpers for the terminal pane (split from terminal/pane.rs for the
//! 400-line cap): shell command, reader thread, grid sizing and the ConPTY
//! startup cursor-query handshake.

use crate::terminal::pane::{SharedWriter, TermDims};
use portable_pty::CommandBuilder;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Reader-side backpressure: stop pulling PTY output while this much is
/// still waiting for the UI thread (a flood would otherwise grow unbounded).
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
/// Upper bound for grid dimensions (PtySize fields are u16).
const MAX_DIM: usize = 1000;

/// Grid size for a measured pixel area, clamped to sane PTY bounds.
pub(crate) fn dims_for_pixels(
    width: f32,
    height: f32,
    cell: crate::terminal::metrics::CellMetrics,
) -> TermDims {
    let cols = (width / cell.width).max(20.) as usize;
    let rows = (height / cell.height).max(8.) as usize;
    TermDims {
        cols: cols.min(MAX_DIM),
        rows: rows.min(MAX_DIM),
    }
}

const DSR_QUERY: &[u8] = b"[6n";
/// ConPTY sends its cursor query before any shell output; past this many
/// bytes, queries come from programs and must reach alacritty, which
/// reports the real cursor position.
const STARTUP_WINDOW: usize = 4096;

/// Answers ConPTY's startup cursor-position query (`ESC[6n`) immediately and
/// strips it, so the handshake is not delayed by the poll interval and
/// alacritty does not answer it a second time. Handles a query split across
/// reads, and stops intercepting after the startup window.
#[derive(Default)]
pub(crate) struct StartupDsr {
    done: bool,
    seen: usize,
    /// Trailing bytes that may begin a query split across reads.
    carry: Vec<u8>,
}

impl StartupDsr {
    /// Filter one read. Returns the bytes to forward to the terminal and
    /// whether the startup query was found (the caller must answer it).
    pub(crate) fn filter(&mut self, chunk: &[u8]) -> (Vec<u8>, bool) {
        if self.done {
            return (chunk.to_vec(), false);
        }
        let mut data = std::mem::take(&mut self.carry);
        data.extend_from_slice(chunk);
        if let Some(pos) = data.windows(DSR_QUERY.len()).position(|w| w == DSR_QUERY) {
            data.drain(pos..pos + DSR_QUERY.len());
            self.done = true;
            return (data, true);
        }
        self.seen += chunk.len();
        if self.seen >= STARTUP_WINDOW {
            self.done = true;
            return (data, false);
        }
        // Hold back a tail that is a proper prefix of the query.
        let keep = (1..DSR_QUERY.len())
            .rev()
            .find(|&k| data.len() >= k && data.ends_with(&DSR_QUERY[..k]))
            .unwrap_or(0);
        self.carry = data.split_off(data.len() - keep);
        (data, false)
    }
}

/// Pump PTY output into `buffer` until EOF or a read error (the master is
/// dropped on shutdown, which unblocks the read).
pub(crate) fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    buffer: Arc<Mutex<Vec<u8>>>,
    dirty: Arc<AtomicBool>,
    writer: SharedWriter,
) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        let mut startup_dsr = StartupDsr::default();
        loop {
            // Backpressure while the UI thread has not drained the backlog;
            // stops for good once the shell is torn down (writer cleared).
            while buffer.lock().map(|b| b.len()).unwrap_or(0) >= MAX_PENDING_BYTES {
                if writer.lock().map(|w| w.is_none()).unwrap_or(true) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let n = match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let (data, query) = startup_dsr.filter(&chunk[..n]);
            if query
                && let Ok(mut lock) = writer.lock()
                && let Some(w) = lock.as_mut()
            {
                let _ = std::io::Write::write_all(w, b"\x1b[1;1R");
                let _ = std::io::Write::flush(w);
            }
            if data.is_empty() {
                continue;
            }
            if let Ok(mut b) = buffer.lock() {
                b.extend_from_slice(&data);
                dirty.store(true, Ordering::Relaxed);
            }
        }
    });
}

pub(crate) fn shell_command(path: &Path) -> CommandBuilder {
    #[cfg(windows)]
    {
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.arg("-NoLogo");
        cmd.cwd(path);
        cmd
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(path);
        cmd
    }
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod tests;
