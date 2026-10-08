use super::store::AppState;
use crate::shared::{desktop_lock, preference_handoff, preferences::Preferences};
use gpui::{AppContext, Context};
use std::time::Duration;

impl AppState {
    pub(super) fn owned_runtime_pids(&self) -> Vec<u32> {
        self.workspaces
            .iter()
            .filter(|ws| ws.pumping)
            .filter_map(|ws| ws.process_id)
            .chain(self.profiles.client.as_ref().map(|c| c.pid()))
            .collect()
    }
    pub(crate) fn activate_profiles(&mut self, cx: &mut Context<Self>) {
        if self.profiles.local_enabled
            || self.profiles.activating
            || self.profiles.closing
            || self.profiles.stopping
            || self.profiles.startup_exit.is_some()
        {
            return;
        }
        self.profiles.activating = true;
        self.profiles.connection_error = None;
        let owned = self.owned_runtime_pids();
        let drained = Preferences::suspend(cx);
        cx.spawn(async move |this, cx| {
            let result = match drained.await {
                Ok(()) => {
                    cx.background_spawn(async move {
                        if desktop_lock::is_desktop_running_except(&owned) {
                            return Err(
                                "Close ZCode Desktop before enabling local management".to_owned()
                            );
                        }
                        crate::backend::services_launch::resolve_mode(None).map(|_| ())
                    })
                    .await
                }
                Err(_) => Err("Preference writer drain failed".into()),
            };
            let _ = this.update(cx, |s, cx| {
                s.profiles.activating = false;
                match result {
                    Ok(()) if !s.profiles.closing => {
                        s.profiles.local_enabled = true;
                        s.profiles.queries.clear();
                        s.read_profiles("user", true, cx);
                    }
                    Ok(()) => s.finish_profile_close(cx),
                    Err(error) => {
                        s.profiles.connection_error = Some(error);
                        s.finish_profile_close(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_profiles(&mut self, cx: &mut Context<Self>) {
        if self.profiles.isolated.is_none()
            && !self.profiles.local_enabled
            && !self.profiles.activating
            && !self.profiles.stopping
            && !self.profiles.closing
        {
            return;
        }
        self.profiles.review_after_close = None;
        self.profiles.recovery_reviewing = false;
        self.profiles.closing = true;
        self.maintain_profiles(cx);
    }

    pub(crate) fn maintain_profiles(&mut self, cx: &mut Context<Self>) {
        if self.profiles.idle_expired() {
            self.profiles.closing = true;
        }
        if !self.profiles.closing
            || self.profiles.stopping
            || self.profiles.activating
            || !self.profiles.idle()
        {
            return;
        }
        self.profiles.stopping = true;
        let mut client = self.profiles.client.take();
        if let Some(client) = &mut client {
            client.shutdown();
        }
        let startup_exit = self.profiles.startup_exit.take();
        let reload_path = Preferences::reload_path(cx);
        let isolated = self.profiles.isolated.is_some();
        self.profiles.generation += 1;
        cx.spawn(async move |this, cx| {
            let (client, startup_exit, result) = cx
                .background_spawn(async move {
                    let result = (|| {
                        if let Some(client) = &client {
                            // 逻辑断连不是进程退出；只有 reaper 完成后才能恢复共享配置写入。
                            client.wait_for_exit(Duration::from_secs(10))?;
                        }
                        if let Some(exit) = &startup_exit {
                            exit.wait_for_exit(Duration::from_secs(10))?;
                        }
                        if isolated {
                            Ok(None)
                        } else {
                            preference_handoff::reload(&reload_path).map(Some)
                        }
                    })();
                    (client, startup_exit, result)
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                s.profiles.stopping = false;
                match result {
                    Ok(disk) => {
                        let review = s.profiles.review_after_close.take();
                        if review.is_none() {
                            if let Some(disk) = disk {
                                Preferences::resume_from(disk, cx);
                            }
                            s.profiles.local_enabled = false;
                        }
                        s.profiles.closing = false;
                        s.profiles.queries.clear();
                        s.profiles.models = None;
                        s.profiles.model_error = None;
                        if let Some(scope) = review {
                            s.profiles.recovery_reviewing = true;
                            s.profiles.models = None;
                            s.read_profiles(&scope, true, cx);
                        }
                    }
                    Err(e) => {
                        s.profiles.client = client;
                        s.profiles.startup_exit = startup_exit;
                        s.profiles.connection_error = Some(e);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn review_profiles(&mut self, cx: &mut Context<Self>) {
        if let Some(m) = &self.profiles.recovery {
            self.profiles.review_after_close = Some(m.scope.clone());
            self.profiles.recovery_observed = false;
            self.profiles.recovery_reviewed = false;
            self.profiles.closing = true;
            self.maintain_profiles(cx);
        }
    }

    fn finish_profile_close(&mut self, cx: &mut Context<Self>) {
        self.profiles.closing = true;
        self.maintain_profiles(cx);
    }
}
