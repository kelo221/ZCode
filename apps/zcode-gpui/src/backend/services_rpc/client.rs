use super::{message::ServiceValue, process, state::Shared};
use futures::channel::oneshot;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

#[derive(Clone, Copy)]
pub(super) struct StartupTimeouts {
    pub hello: Duration,
    pub initialize: Duration,
}

/// Failure before spawn has no observer. After spawn, callers must retain the
/// observer and keep preference admission suspended until it returns Ok.
#[derive(Debug)]
pub struct ServiceSpawnError {
    pub message: String,
    pub exit_observer: Option<ServiceExitObserver>,
}
impl ServiceSpawnError {
    pub(super) fn before_spawn(message: String) -> Self {
        Self {
            message,
            exit_observer: None,
        }
    }
}
impl std::fmt::Display for ServiceSpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ServiceSpawnError {}

/// Read-only root/tree exit proof, independent of client lifetime or RPC close.
#[derive(Clone)]
pub struct ServiceExitObserver {
    shared: Arc<Shared>,
}
impl std::fmt::Debug for ServiceExitObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceExitObserver")
            .finish_non_exhaustive()
    }
}
impl ServiceExitObserver {
    pub(super) fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
    /// Background-only wait. A timeout can be retried; failed OS observation is
    /// permanently unsafe and never permits preferences to resume or replacement.
    pub fn wait_for_exit(&self, timeout: Duration) -> Result<(), String> {
        self.shared.exit.wait(timeout)
    }
}

/// Owned process client. Construct on a background thread, share via application
/// state if needed; cloning this owner would make shutdown ownership ambiguous.
pub struct ServiceClient {
    shared: Arc<Shared>,
    pid: u32,
}

/// Dropping the receiver does not cancel a committed mutation. Explicit cancel
/// rejects locally and sends advisory PromiseCancel; it cannot promise rollback.
pub struct ServiceCall {
    pub id: u32,
    pub receiver: oneshot::Receiver<Result<ServiceValue, String>>,
    owner: Weak<Shared>,
}
impl ServiceCall {
    /// Wait without blocking the UI; the caller owns the timer/deadline policy.
    pub async fn wait(
        self,
        deadline: impl std::future::Future<Output = ()> + Unpin,
    ) -> Result<ServiceValue, String> {
        let Self {
            id,
            receiver,
            owner,
        } = self;
        match futures::future::select(receiver, deadline).await {
            futures::future::Either::Left((result, _)) => {
                result.map_err(|_| "Services RPC result channel closed".to_string())?
            }
            futures::future::Either::Right(((), receiver)) => {
                if let Some(owner) = owner.upgrade()
                    && Self::cancel_on(&owner, id).is_err()
                {
                    // 超时必须释放 pending；取消排队失败时关闭传输，不伪造远端回滚。
                    owner.close("Services RPC timeout cancellation could not be admitted");
                }
                drop(receiver);
                Err(
                    "Services RPC timed out; mutation outcome is unknown (no replay or rollback)"
                        .into(),
                )
            }
        }
    }
    fn cancel_on(owner: &Shared, id: u32) -> Result<bool, String> {
        owner.cancel(id)
    }
    /// Returns true exactly when this call was pending and cancellation admitted.
    #[cfg(test)]
    pub fn cancel(&self) -> Result<bool, String> {
        self.owner
            .upgrade()
            .map_or(Ok(false), |owner| Self::cancel_on(&owner, self.id))
    }
}

impl ServiceClient {
    /// Caller supplies validated runtime, sanitized explicit environment and
    /// isolated cwd. No environment inheritance, discovery or cwd fallback.
    #[cfg(test)]
    pub fn spawn(
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
        cwd: &Path,
    ) -> Result<Self, String> {
        Self::spawn_observed(program, args, envs, cwd).map_err(Self::observe_spawn_error)
    }
    /// Management entrypoint: startup errors retain the preference handoff gate.
    pub fn spawn_observed(
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
        cwd: &Path,
    ) -> Result<Self, ServiceSpawnError> {
        Self::spawn_with_timeouts(
            program,
            args,
            envs,
            cwd,
            StartupTimeouts {
                hello: Duration::from_secs(10),
                initialize: Duration::from_secs(30),
            },
        )
    }
    #[cfg(test)]
    fn observe_spawn_error(error: ServiceSpawnError) -> String {
        if let Some(observer) = error.exit_observer
            && let Err(cleanup) = observer.wait_for_exit(super::process_tree::CLEANUP_TIMEOUT)
        {
            return format!("{}; {cleanup}", error.message);
        }
        error.message
    }
    pub(super) fn spawn_with_timeouts(
        program: &Path,
        args: &[String],
        envs: &[(String, String)],
        cwd: &Path,
        timeouts: StartupTimeouts,
    ) -> Result<Self, ServiceSpawnError> {
        let (shared, pid) = process::spawn(program, args, envs, cwd, timeouts)?;
        Ok(Self { shared, pid })
    }
    /// Internal transport primitive; application wrappers enforce method policy.
    pub fn call(
        &self,
        channel: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<super::ServiceCall, String> {
        let (id, receiver) = self.shared.admit(channel, method, args)?;
        Ok(ServiceCall {
            id,
            receiver,
            owner: Arc::downgrade(&self.shared),
        })
    }
    pub fn is_alive(&self) -> bool {
        self.shared.lock().alive
    }
    pub fn pending_count(&self) -> usize {
        self.shared.lock().pending.len()
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn exit_observer(&self) -> ServiceExitObserver {
        ServiceExitObserver::new(self.shared.clone())
    }
    pub(crate) fn wait_for_exit(&self, timeout: Duration) -> Result<(), String> {
        self.exit_observer().wait_for_exit(timeout)
    }
    /// Settles outstanding calls immediately; pipe teardown/reaping is off-thread.
    pub fn shutdown(&mut self) {
        self.shared.close("Services RPC client shut down");
    }
}
impl Drop for ServiceClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}
