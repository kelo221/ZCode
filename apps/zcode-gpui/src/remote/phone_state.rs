//! Phone build (`remote-client`): `AppState` starts without local agents,
//! takes its workspace list from the paired desktop and connects each
//! workspace through `remote::client::connect`.

use crate::app::store::AppState;
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};
use crate::remote::client::{RemoteTarget, list_workspaces, pair};
use crate::remote::handshake::RemoteWorkspace;
use crate::remote::link::PairLink;
use gpui::{AppContext, Context};
use std::time::Duration;

const LINK_POLL: Duration = Duration::from_millis(500);
const LIST_RETRY: Duration = Duration::from_secs(5);
const MAX_RECONNECT_DELAY_MS: u64 = 15_000;

#[derive(Default)]
pub(crate) struct PhoneClient {
    /// True on the phone build: workspaces come from the desktop.
    pub(crate) enabled: bool,
    pub(crate) target: Option<RemoteTarget>,
    pub(crate) status: String,
    /// Bumped when the pairing changes, so a stale list fetch is ignored.
    generation: u64,
}

pub(crate) const NOT_PAIRED: &str = "Not paired with a desktop. On the desktop, open Settings › Phone, turn it on and scan the QR code with this phone.";

fn device_name() -> String {
    if cfg!(target_os = "android") {
        "Android phone".into()
    } else {
        "Phone".into()
    }
}

impl AppState {
    pub(crate) fn start_remote_client(&mut self, cx: &mut Context<Self>) {
        self.phone_client.enabled = true;
        self.phone_client.target = RemoteTarget::load();
        match self.phone_client.target.clone() {
            Some(target) => self.fetch_remote_workspaces(target, cx),
            None => {
                self.phone_client.status = "not paired".into();
                self.push_error(NOT_PAIRED.into());
            }
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LINK_POLL).await;
                let alive = this
                    .update(cx, |state, cx| {
                        if let Some(link) = crate::remote::take_pending_link() {
                            state.pair_with_link(&link, cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn fetch_remote_workspaces(&mut self, target: RemoteTarget, cx: &mut Context<Self>) {
        let generation = self.phone_client.generation;
        self.phone_client.status = "connecting to desktop".into();
        cx.spawn(async move |this, cx| {
            loop {
                let request = target.clone();
                let result = cx
                    .background_spawn(async move { list_workspaces(&request) })
                    .await;
                let retry = this
                    .update(cx, |state, cx| {
                        if state.phone_client.generation != generation {
                            return false;
                        }
                        match result {
                            Ok(list) => {
                                state.apply_remote_workspaces(&target, list, cx);
                                false
                            }
                            Err(error) => {
                                state.phone_client.status = format!("desktop offline ({error})");
                                cx.notify();
                                true
                            }
                        }
                    })
                    .unwrap_or(false);
                if !retry {
                    break;
                }
                cx.background_executor().timer(LIST_RETRY).await;
            }
        })
        .detach();
    }

    pub(crate) fn pair_with_link(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(link) = PairLink::parse(text) else {
            self.push_error("That is not a ZCode GPUI pairing link.".into());
            cx.notify();
            return;
        };
        self.phone_client.status = format!("pairing with {}", link.host);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let (target, list) = pair(&link, &device_name())?;
                    target.save()?;
                    Ok::<_, String>((target, list))
                })
                .await;
            let _ = this.update(cx, |state, cx| match result {
                Ok((target, list)) => {
                    state.forget_remote_workspaces();
                    state.errors.retain(|e| e != NOT_PAIRED);
                    state.phone_client.target = Some(target.clone());
                    state.apply_remote_workspaces(&target, list, cx);
                }
                Err(error) => {
                    state.phone_client.status = "pairing failed".into();
                    state.push_error(format!("Pairing failed: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn forget_desktop(&mut self, cx: &mut Context<Self>) {
        RemoteTarget::forget();
        self.forget_remote_workspaces();
        self.phone_client.target = None;
        self.phone_client.status = "not paired".into();
        cx.notify();
    }

    fn forget_remote_workspaces(&mut self) {
        self.phone_client.generation += 1;
        let keys: Vec<String> = self.workspaces.iter().map(|w| w.key.clone()).collect();
        for key in &keys {
            self.unload_workspace(key);
        }
        self.workspaces.clear();
        self.active_workspace = None;
        self.active = None;
        self.conversations.clear();
    }

    fn apply_remote_workspaces(
        &mut self,
        target: &RemoteTarget,
        list: Vec<RemoteWorkspace>,
        cx: &mut Context<Self>,
    ) {
        for remote in list {
            if self.ws(&remote.key).is_some() {
                continue;
            }
            let conversation = remote.purpose == "conversation";
            let handle = WorkspaceHandle::remote(
                remote.key,
                if conversation {
                    "Tasks".into()
                } else {
                    remote.display
                },
                if conversation {
                    WorkspacePurpose::Conversation
                } else {
                    WorkspacePurpose::Project
                },
                target.clone(),
            );
            if conversation {
                self.workspaces.insert(0, handle);
            } else {
                self.workspaces.push(handle);
            }
        }
        self.phone_client.status = format!("paired with {}", target.host);
        if self.active_workspace.is_none() {
            self.active_workspace = self
                .workspaces
                .iter()
                .find(|w| w.purpose == WorkspacePurpose::Project)
                .or(self.workspaces.first())
                .map(|w| w.key.clone());
            if let Some(key) = self.active_workspace.clone() {
                self.spawn_workspace(&key, cx);
            }
        }
        cx.notify();
    }

    /// A remote workspace's desktop link dropped. It never runs out of
    /// candidates: retry with backoff while the app is open.
    pub(crate) fn handle_remote_exit(&mut self, ws_key: &str, cx: &mut Context<Self>) -> bool {
        let Some(ws) = self.ws(ws_key) else {
            return false;
        };
        let was_started = ws.started;
        let attempts = ws.restart_attempts;
        self.reset_connection_state(ws_key);
        let delay_ms = (1000u64 << attempts.min(4)).min(MAX_RECONNECT_DELAY_MS);
        let generation = match self.ws_mut(ws_key) {
            Some(ws) => {
                ws.kill = None;
                ws.restart_attempts = attempts.saturating_add(1);
                ws.status = if was_started {
                    "reconnecting to desktop".into()
                } else {
                    "desktop offline".into()
                };
                ws.generation
            }
            None => return false,
        };
        let key = ws_key.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay_ms))
                .await;
            let _ = this.update(cx, |state, cx| {
                if state
                    .ws(&key)
                    .is_some_and(|w| w.generation == generation && !w.pumping)
                {
                    state.spawn_workspace(&key, cx);
                }
            });
        })
        .detach();
        cx.notify();
        true
    }
}
