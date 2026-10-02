//! PTY I/O helpers for the terminal pane (split from terminal/pane.rs for the
//! 400-line cap): shell command, reader thread, grid sizing and the ConPTY
//! startup cursor-query handshake.

use crate::terminal::pane::{SharedWriter, TermDims, CELL_H, CELL_W};
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
pub(crate) fn dims_for_pixels(width: f32, height: f32) -> TermDims {
    let cols = ((width - 8.) / CELL_W).max(20.) as usize;
    let rows = ((height - 8.) / CELL_H).max(8.) as usize;
    TermDims {
        cols: cols.min(MAX_DIM),
        rows: rows.min(MAX_DIM),
    }
}

/// Answer the first ConPTY cursor-position query (`ESC[6n`) immediately and
/// strip it, so the startup handshake is not delayed by the poll interval and
/// alacritty does not answer it a second time. Later queries are left to
/// alacritty, which reports the real cursor position.
pub(crate) fn take_startup_dsr(data: &mut Vec<u8>, answered: &mut bool) -> bool {
    if *answered {
        return false;
    }
    let Some(pos) = data.windows(4).position(|w| w == b"\x1b[6n") else {
        return false;
    };
    data.drain(pos..pos + 4);
    *answered = true;
    true
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
        let mut answered_dsr = false;
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
            let mut data = chunk[..n].to_vec();
            if take_startup_dsr(&mut data, &mut answered_dsr)
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
