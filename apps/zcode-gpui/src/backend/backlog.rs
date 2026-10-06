//! Producer/consumer backlog accounting and bounded line admission for
//! backend connections (split from backend/conn.rs for the 400-line cap).
//!
//! Protocol (stdout) and diagnostics (stderr) each get an independent
//! EventBacklog: a stderr storm can never latch the protocol overflow flag or
//! consume the protocol byte budget (follow-up review finding 6).

use crate::backend::launcher::ConnEvent;
use std::io::{BufRead, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Hard byte bound on queued protocol lines. Crossing it means the CLI
/// ignored the flow pause, so the connection is treated as desynchronized
/// (dropped lines leave holes no resync can repair) and restarted.
pub(crate) const MAX_PROTOCOL_BACKLOG_BYTES: usize = 64 * 1024 * 1024;
/// Independent lossy bound for agent stderr lines (log storms drop logs).
pub(crate) const MAX_LOG_BACKLOG_BYTES: usize = 4 * 1024 * 1024;
/// A single line above twice the physical frame budget can never be valid —
/// the reader discards it before it can grow further.
pub(crate) const MAX_RAW_LINE_BYTES: usize = crate::backend::wire::MAX_FRAME_BYTES * 2;

#[derive(Default)]
struct Backlog {
    events: AtomicUsize,
    bytes: AtomicUsize,
    overflow: AtomicBool,
}

/// Producer/consumer backlog accounting for one stream. The stdout/stderr
/// producer threads reserve capacity per event (atomically — two producers
/// can never overshoot the cap); the pump releases after processing.
#[derive(Clone)]
pub(crate) struct EventBacklog(Arc<Inner>);

struct Inner {
    inner: Backlog,
    max_bytes: usize,
}

impl EventBacklog {
    pub(crate) fn new(max_bytes: usize) -> Self {
        Self(Arc::new(Inner {
            inner: Backlog::default(),
            max_bytes,
        }))
    }

    /// Queued events (producer-side; includes the event being processed).
    pub(crate) fn depth(&self) -> usize {
        self.0.inner.events.load(Ordering::Relaxed)
    }

    /// Queued bytes (for byte-based flow watermarks).
    pub(crate) fn bytes(&self) -> usize {
        self.0.inner.bytes.load(Ordering::Relaxed)
    }

    /// Take and clear the overflow flag: true when events were dropped.
    pub(crate) fn take_overflow(&self) -> bool {
        self.0.inner.overflow.swap(false, Ordering::Relaxed)
    }

    /// Atomically reserve `len` bytes and count one event. Returns false
    /// (and latches overflow) when the hard cap would be exceeded: the
    /// caller must drop the event. The compare-exchange loop means two
    /// producer threads can never both pass where only one would fit.
    pub(crate) fn reserve(&self, len: usize) -> bool {
        let bytes = &self.0.inner.bytes;
        let mut current = bytes.load(Ordering::Relaxed);
        loop {
            let Some(next) = current.checked_add(len) else {
                self.0.inner.overflow.store(true, Ordering::Relaxed);
                return false;
            };
            if next > self.0.max_bytes {
                self.0.inner.overflow.store(true, Ordering::Relaxed);
                return false;
            }
            match bytes.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => {
                    self.0.inner.events.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Release one admitted event; returns the remaining depth.
    pub(crate) fn release(&self, len: usize) -> usize {
        self.0.inner.bytes.fetch_sub(len, Ordering::Relaxed);
        self.0.inner.events.fetch_sub(1, Ordering::Relaxed) - 1
    }
}

/// Admit one event onto the queue unless the stream's hard byte bound is hit
/// (then the event is dropped and overflow is latched). When the receiver is
/// gone the reservation is rolled back so the backlog stays truthful.
pub(crate) fn push_event(
    backlog: &EventBacklog,
    tx: &futures::channel::mpsc::UnboundedSender<ConnEvent>,
    ev: ConnEvent,
    len: usize,
) -> bool {
    if !backlog.reserve(len) {
        return true; // overflow: drop the event, keep draining the pipe
    }
    if tx.unbounded_send(ev).is_err() {
        backlog.release(len);
        return false;
    }
    true
}

/// Result of a bounded protocol-line read: the working set never grows
/// past `max + 1` bytes (review finding 6).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BoundedLine {
    Eof,
    Overlong,
    Line(Vec<u8>),
}

/// Discard the remainder of an overlong line without consuming the next one
/// (`Read::read` into a 4 KiB sink would swallow a following protocol line
/// when both fit in one chunk).
pub(crate) fn discard_rest_of_line(reader: &mut impl BufRead) {
    loop {
        let buf = match reader.fill_buf() {
            Ok([]) => break,
            Ok(b) => b,
            Err(_) => break,
        };
        if let Some(i) = buf.iter().position(|&c| c == b'\n') {
            reader.consume(i + 1);
            break;
        }
        let n = buf.len();
        reader.consume(n);
    }
}

pub(crate) fn read_bounded_line(reader: &mut impl BufRead, max: usize) -> BoundedLine {
    let mut buf = Vec::new();
    let mut limited = reader.take((max + 1) as u64);
    match limited.read_until(b'\n', &mut buf) {
        Ok(0) | Err(_) => return BoundedLine::Eof,
        Ok(_) => {}
    }
    if buf.last() != Some(&b'\n') {
        discard_rest_of_line(reader);
        return BoundedLine::Overlong;
    }
    buf.pop();
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    BoundedLine::Line(buf)
}

#[cfg(test)]
#[path = "backlog_tests.rs"]
mod tests;
