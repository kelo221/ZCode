use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
pub(super) struct ProcessExit {
    outcome: Mutex<Option<Result<(), String>>>,
    wake: Condvar,
}
impl ProcessExit {
    // 逻辑关闭和 kill 请求不是退出证据；只有 root wait 与子树核验共同成功才放行。
    pub(super) fn complete(&self, outcome: Result<(), String>) {
        let mut current = self.outcome.lock().unwrap_or_else(|e| e.into_inner());
        if current.is_none() {
            *current = Some(outcome);
            self.wake.notify_all();
        }
    }
    pub(super) fn wait(&self, timeout: Duration) -> Result<(), String> {
        let outcome = self.outcome.lock().unwrap_or_else(|e| e.into_inner());
        let (outcome, _) = self
            .wake
            .wait_timeout_while(outcome, timeout, |done| done.is_none())
            .map_err(
                |_| "Services process exit observation failed; preferences remain suspended",
            )?;
        outcome.clone().unwrap_or_else(|| {
            Err("Services process tree exit not verified; preferences remain suspended".into())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_close_is_not_observed_exit() {
        let exit = ProcessExit::default();
        assert!(exit.wait(Duration::ZERO).is_err());
        exit.complete(Ok(()));
        assert!(exit.wait(Duration::ZERO).is_ok());
    }
    #[test]
    fn failed_observation_cannot_be_replaced_by_success() {
        let exit = ProcessExit::default();
        exit.complete(Err("root wait failed; preferences remain suspended".into()));
        exit.complete(Ok(()));
        assert!(
            exit.wait(Duration::ZERO)
                .unwrap_err()
                .contains("root wait failed")
        );
    }
}
