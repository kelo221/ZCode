//! Sole admission, pending-result and terminal-state owner for a connection.
use super::message::{Message, ServiceValue, cancel_frame, request_frame};
use futures::channel::oneshot;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard};

pub(super) const MAX_PENDING_CALLS: usize = 64;
pub(super) const MAX_QUEUED_FRAMES: usize = 128;
pub(super) const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
type Reply = oneshot::Sender<Result<ServiceValue, String>>;
type Kill = Box<dyn FnOnce() + Send>;

pub(super) struct State {
    pub alive: bool,
    pub initialized: bool,
    pub next_id: u32,
    pub pending: HashMap<u32, Reply>,
    pub queue: VecDeque<Vec<u8>>,
    pub queued_bytes: usize,
    kill: Option<Kill>,
}

impl State {
    pub(super) fn enqueue(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        if !self.alive {
            return Err("Services RPC connection closed".into());
        }
        if self.queue.len() >= MAX_QUEUED_FRAMES
            || self.queued_bytes.saturating_add(bytes.len()) > MAX_QUEUED_BYTES
        {
            return Err("Services RPC outbound queue limit exceeded".into());
        }
        self.queued_bytes += bytes.len();
        self.queue.push_back(bytes);
        Ok(())
    }
}

pub(super) struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    pub(super) exit: super::exit::ProcessExit,
}
impl Shared {
    pub(super) fn new(kill: Option<Kill>) -> Self {
        Self {
            state: Mutex::new(State {
                alive: true,
                initialized: false,
                next_id: 0,
                pending: HashMap::new(),
                queue: VecDeque::new(),
                queued_bytes: 0,
                kill,
            }),
            wake: Condvar::new(),
            exit: super::exit::ProcessExit::default(),
        }
    }
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub(super) fn enqueue(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.lock().enqueue(bytes)?;
        self.wake.notify_one();
        Ok(())
    }
    pub(super) fn admit(
        &self,
        channel: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<(u32, oneshot::Receiver<Result<ServiceValue, String>>), String> {
        if channel.is_empty() || method.is_empty() || channel.len() > 256 || method.len() > 256 {
            return Err("Services RPC invalid channel or method".into());
        }
        let mut state = self.lock();
        if !state.alive || !state.initialized {
            return Err("Services RPC connection not ready".into());
        }
        if state.pending.len() >= MAX_PENDING_CALLS {
            return Err("Services RPC pending call limit exceeded".into());
        }
        if state.next_id > i32::MAX as u32 {
            return Err("Services RPC request id exhausted".into());
        }
        let id = state.next_id;
        let frame = request_frame(id, channel, method, args)?;
        let (tx, rx) = oneshot::channel();
        // 同一锁内先排队再登记 pending，防止 EOF 或取消抢在登记前导致永远 pending。
        state.enqueue(frame)?;
        state.next_id += 1;
        state.pending.insert(id, tx);
        drop(state);
        self.wake.notify_one();
        Ok((id, rx))
    }
    pub(super) fn cancel(&self, id: u32) -> Result<bool, String> {
        let mut state = self.lock();
        if !state.pending.contains_key(&id) {
            return Ok(false);
        }
        // 取消先进入同一 FIFO；队列拒绝时不移除 pending，不伪造取消已经送达。
        state.enqueue(cancel_frame(id)?)?;
        if let Some(tx) = state.pending.remove(&id) {
            let _ = tx.send(Err(
                "Services RPC cancelled; mutation rollback is not guaranteed".into(),
            ));
        }
        drop(state);
        self.wake.notify_one();
        Ok(true)
    }
    pub(super) fn take_frame(&self) -> Option<Vec<u8>> {
        let mut state = self.lock();
        loop {
            if !state.alive {
                return None;
            }
            if let Some(bytes) = state.queue.pop_front() {
                state.queued_bytes -= bytes.len();
                return Some(bytes);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
    pub(super) fn response(&self, message: Message) -> Result<bool, String> {
        let mut state = self.lock();
        if !state.alive {
            return Ok(false);
        }
        match message {
            Message::Initialize if !state.initialized => {
                state.initialized = true;
                Ok(true)
            }
            Message::Initialize => Err("Services RPC duplicate Initialize".into()),
            Message::Success(id, value) => Self::settle(&mut state, id, Ok(value)),
            Message::Failure(id, error) => Self::settle(&mut state, id, Err(error)),
        }
    }
    fn settle(
        state: &mut State,
        id: u32,
        result: Result<ServiceValue, String>,
    ) -> Result<bool, String> {
        if !state.initialized || id >= state.next_id {
            return Err("Services RPC uncorrelated response".into());
        }
        if let Some(tx) = state.pending.remove(&id) {
            let _ = tx.send(result);
        }
        Ok(false)
    }
    pub(super) fn close(&self, reason: &str) {
        let (pending, kill) = {
            let mut state = self.lock();
            if !state.alive {
                return;
            }
            state.alive = false;
            state.queue.clear();
            state.queued_bytes = 0;
            (std::mem::take(&mut state.pending), state.kill.take())
        };
        self.wake.notify_all();
        for (_, tx) in pending {
            let _ = tx.send(Err(reason.to_owned()));
        }
        // 不能持 admission 锁调用进程清理；清理可能等待 OS，reader/cancel 不得死锁。
        if let Some(kill) = kill {
            kill();
        }
    }
}
