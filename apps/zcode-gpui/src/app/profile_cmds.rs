use crate::app::profile_state::{ProfileMutation, ProfileRequest};
use crate::app::store::AppState;
use crate::app::subagent_profiles::{AgentSummary, WorkspaceContext};
use crate::backend::services_rpc::ServiceClient;
use gpui::{AppContext, Context};
use serde_json::{Value, json};
use std::path::Path;

impl AppState {
    pub(crate) fn profile_workspace(&self, scope: &str) -> Option<String> {
        if scope == "user" {
            if let Some(isolated) = &self.profiles.isolated {
                return Some(isolated.workspace().to_string_lossy().into_owned());
            }
            return self
                .active_ws_key()
                .and_then(|key| self.ws(&key))
                .or_else(|| self.workspaces.first())
                .map(|ws| ws.path.to_string_lossy().into_owned());
        }
        let workspace = self.ws(scope)?;
        self.profiles
            .isolated
            .as_ref()
            .is_none_or(|i| i.contains_known_path(&workspace.path))
            .then(|| workspace.path.to_string_lossy().into_owned())
    }

    pub(crate) fn profile_context(&self, scope: &str) -> Option<WorkspaceContext> {
        let path = self.profile_workspace(scope)?;
        let identity = (scope != "user")
            .then(|| scope.trim().to_owned())
            .filter(|key| !key.is_empty());
        Some(WorkspaceContext {
            workspace_path: Some(path),
            workspace_identity: identity,
        })
    }

    pub(crate) fn profile_list_params(&self, scope: &str) -> Option<Value> {
        let context = self.profile_context(scope)?;
        let mut params = json!({"workspacePath":context.workspace_path,"mode":if scope == "user" {"settingsUserOnly"} else {"allRuntimeScopes"}});
        if let Some(identity) = context.workspace_identity {
            params["workspaceIdentity"] = json!(identity);
        }
        Some(params)
    }

    pub(crate) fn read_profiles(&mut self, scope: &str, refresh: bool, cx: &mut Context<Self>) {
        if !self.profiles.ready() || self.profile_workspace(scope).is_none() {
            return;
        }
        let query = self.profiles.queries.entry(scope.into()).or_default();
        if query.loading || (!refresh && (query.snapshot.is_some() || query.error.is_some())) {
            return;
        }
        query.loading = true;
        query.serial += 1;
        query.error = None;
        let serial = query.serial;
        self.enqueue_profile_request(
            ProfileRequest::List {
                scope: scope.into(),
                serial,
            },
            cx,
        );
        if self.profiles.models.is_none() && !self.profiles.models_loading {
            self.profiles.models_loading = true;
            self.enqueue_profile_request(ProfileRequest::Models, cx);
        }
    }

    pub(crate) fn submit_profile_mutation(
        &mut self,
        receipt: String,
        scope: String,
        method: &str,
        params: Value,
        baseline: Option<AgentSummary>,
        cx: &mut Context<Self>,
    ) {
        if !self.profiles.ready()
            || self.profile_workspace(&scope).is_none()
            || self.profiles.mutation_pending
            || self.profiles.uncertain
        {
            return;
        }
        if !matches!(
            method,
            "createAgent"
                | "updateAgent"
                | "deleteAgent"
                | "setEnabled"
                | "setBuiltInModelOverride"
                | "setPluginAgentModelOverride"
        ) {
            return;
        }
        self.profiles.mutation_pending = true;
        self.profiles.outcomes.remove(&receipt);
        let origin = self.profile_context(&scope).unwrap_or_default();
        let request = ProfileMutation {
            origin,
            receipt,
            scope,
            method: method.into(),
            params,
            baseline,
        };
        self.enqueue_profile_request(ProfileRequest::Preflight(request), cx);
    }

    pub(crate) fn enqueue_profile_request(
        &mut self,
        request: ProfileRequest,
        cx: &mut Context<Self>,
    ) {
        if self.profiles.stopping || self.profiles.startup_exit.is_some() {
            self.settle_profile_request(
                request,
                Err("Host cleanup must complete before reconnection".into()),
                cx,
            );
            return;
        }
        self.profiles.last_activity = Some(std::time::Instant::now());
        if self
            .profiles
            .client
            .as_ref()
            .is_some_and(ServiceClient::is_alive)
        {
            self.send_profile_request(request, cx);
            return;
        }
        if self.profiles.client.is_some()
            && self.profiles.mutation_pending
            && matches!(&request, ProfileRequest::Commit(_))
        {
            self.settle_profile_request(
                request,
                Err("Services connection closed; refresh after the write settles".into()),
                cx,
            );
            return;
        }
        if self.profiles.client.is_some() {
            self.settle_profile_request(
                request,
                Err("Services connection closed; reopen manager to reconnect".into()),
                cx,
            );
            self.profiles.closing = true;
            self.maintain_profiles(cx);
            return;
        }
        self.profiles.queue.push_back(request);
        if self.profiles.starting {
            return;
        }
        let isolated = self.profiles.isolated.clone();
        let Some(cwd) = isolated
            .as_ref()
            .map(|i| i.workspace())
            .or_else(|| self.workspaces.first().map(|w| w.path.clone()))
        else {
            return;
        };
        self.profiles.starting = true;
        self.profiles.client = None;
        self.profiles.generation += 1;
        let generation = self.profiles.generation;
        let owned = self.owned_runtime_pids();
        cx.spawn(async move |this, cx| {
            let (result, startup_exit) = cx
                .background_spawn(async move {
                    if isolated.is_none()
                        && crate::shared::desktop_lock::is_desktop_running_except(&owned)
                    {
                        return (
                            Err("Close ZCode Desktop before local management".into()),
                            None,
                        );
                    }
                    let launch =
                        match crate::backend::services_launch::resolve_mode(isolated.as_deref()) {
                            Ok(launch) => launch,
                            Err(e) => return (Err(e), None),
                        };
                    match ServiceClient::spawn_observed(
                        Path::new(&launch.program),
                        &launch.args,
                        &launch.envs,
                        &cwd,
                    ) {
                        Ok(client) => (Ok(client), None),
                        Err(crate::backend::services_rpc::ServiceSpawnError {
                            message,
                            exit_observer,
                        }) => (Err(message), exit_observer),
                    }
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                if state.profiles.generation != generation {
                    return;
                }
                state.profiles.starting = false;
                match result {
                    Ok(client) => {
                        state.push_log(format!(
                            "owned Settings Host started (pid {})",
                            client.pid()
                        ));
                        state.profiles.client = Some(client);
                        state.profiles.connection_error = None;
                        while let Some(request) = state.profiles.queue.pop_front() {
                            state.send_profile_request(request, cx);
                        }
                    }
                    Err(error) => {
                        state.profiles.startup_exit = startup_exit;
                        state.profiles.closing = true;
                        state.profiles.connection_error =
                            Some(crate::shared::redact::scrub(&error));
                        while let Some(request) = state.profiles.queue.pop_front() {
                            state.settle_profile_request(request, Err(error.clone()), cx);
                        }
                    }
                }
                state.maintain_profiles(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn send_profile_request(&mut self, request: ProfileRequest, cx: &mut Context<Self>) {
        let (channel, method, args) = match &request {
            ProfileRequest::List { scope, .. } => (
                "subagents",
                "list",
                self.profile_list_params(scope).map(|v| vec![v]),
            ),
            ProfileRequest::Preflight(mutation) => (
                "subagents",
                "list",
                Some(vec![{
                    let mut params = json!({"workspacePath": mutation.origin.workspace_path, "mode": if mutation.scope == "user" {"settingsUserOnly"} else {"allRuntimeScopes"}});
                    if let Some(identity) = &mutation.origin.workspace_identity {
                        params["workspaceIdentity"] = json!(identity);
                    }
                    params
                }]),
            ),
            ProfileRequest::Models => ("model-selection", "getView", Some(vec![])),
            ProfileRequest::Commit(mutation) => (
                "subagents",
                mutation.method.as_str(),
                Some(vec![mutation.params.clone()]),
            ),
        };
        let Some(args) = args else {
            self.settle_profile_request(request, Err("Profile scope is unavailable".into()), cx);
            return;
        };
        let call = self
            .profiles
            .client
            .as_ref()
            .ok_or("Services Host is unavailable")
            .and_then(|client| {
                client
                    .call(channel, method, args)
                    .map_err(|_| "Services request could not be sent")
            });
        let call = match call {
            Ok(call) => call,
            Err(error) => {
                self.settle_profile_request(request, Err(error.into()), cx);
                return;
            }
        };
        let generation = self.profiles.generation;
        cx.spawn(async move |this, cx| {
            let result = call
                .wait(
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(30)),
                )
                .await;
            let _ = this.update(cx, |state, cx| {
                if state.profiles.generation == generation {
                    state.settle_profile_request(request, result, cx);
                }
            });
        })
        .detach();
    }
}
