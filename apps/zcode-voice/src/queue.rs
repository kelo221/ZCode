//! Bounded speech queue shared by the stdin reader and the speaking worker.

use crate::protocol::SpeakRequest;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

pub const QUEUE_CAPACITY: usize = 3;

#[derive(Default)]
struct Inner {
    jobs: VecDeque<SpeakRequest>,
    closed: bool,
}

#[derive(Default)]
pub struct SpeechQueue {
    inner: Mutex<Inner>,
    ready: Condvar,
}

impl SpeechQueue {
    /// Enqueues a job; when full, the oldest waiting job is dropped and returned
    /// so the caller can report it.
    pub fn push(&self, job: SpeakRequest) -> Option<SpeakRequest> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let dropped = if inner.jobs.len() >= QUEUE_CAPACITY {
            inner.jobs.pop_front()
        } else {
            None
        };
        inner.jobs.push_back(job);
        self.ready.notify_one();
        dropped
    }

    /// Removes every waiting job and returns them.
    pub fn clear(&self) -> Vec<SpeakRequest> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.jobs.drain(..).collect()
    }

    /// Blocks until a job is available; `None` once closed and drained.
    pub fn pop(&self) -> Option<SpeakRequest> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(job) = inner.jobs.pop_front() {
                return Some(job);
            }
            if inner.closed {
                return None;
            }
            inner = self.ready.wait(inner).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn close(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.closed = true;
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str) -> SpeakRequest {
        SpeakRequest {
            id: id.into(),
            text: "hi".into(),
            voice: None,
            speed: None,
            volume: None,
        }
    }

    #[test]
    fn drops_oldest_when_full() {
        let queue = SpeechQueue::default();
        for id in ["a", "b", "c"] {
            assert!(queue.push(job(id)).is_none());
        }
        assert_eq!(queue.push(job("d")).map(|j| j.id), Some("a".into()));
        assert_eq!(queue.pop().map(|j| j.id), Some("b".into()));
    }

    #[test]
    fn clear_returns_waiting_jobs() {
        let queue = SpeechQueue::default();
        queue.push(job("a"));
        queue.push(job("b"));
        let ids: Vec<_> = queue.clear().into_iter().map(|j| j.id).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn pop_returns_none_after_close() {
        let queue = SpeechQueue::default();
        queue.push(job("a"));
        queue.close();
        assert_eq!(queue.pop().map(|j| j.id), Some("a".into()));
        assert!(queue.pop().is_none());
    }
}
