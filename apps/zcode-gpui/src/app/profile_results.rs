use crate::app::profile_state::{ProfileMutation, ProfileRequest};
use crate::app::store::AppState;
use crate::app::subagent_profiles::{AgentsListResult, ModelSelectionView};
use crate::backend::services_rpc::ServiceValue;
use gpui::{AppContext, Context};
use serde_json::Value;

impl AppState {
    pub(crate) fn settle_profile_request(
        &mut self,
        request: ProfileRequest,
        result: Result<ServiceValue, String>,
        cx: &mut Context<Self>,
    ) {
        match request {
            ProfileRequest::List { scope, serial } => {
                let result = json_result(result).and_then(|value| {
                    serde_json::from_value::<AgentsListResult>(value)
                        .map_err(|_| "Invalid Subagents service inventory".into())
                });
                if let Some(query) = self.profiles.queries.get_mut(&scope)
                    && query.serial == serial
                {
                    query.loading = false;
                    match result {
                        Ok(snapshot) => {
                            if let Some(m) = &self.profiles.recovery
                                && m.scope == scope
                                && self.profiles.recovery_reviewing
                            {
                                self.profiles.recovery_observed =
                                    super::profile_recovery::observed(m, &snapshot);
                                self.profiles.recovery_reviewed = true;
                                self.profiles.recovery_reviewing = false;
                            }
                            query.snapshot = Some(snapshot);
                            query.error = None;
                        }
                        Err(error) => {
                            if self
                                .profiles
                                .recovery
                                .as_ref()
                                .is_some_and(|m| m.scope == scope)
                            {
                                self.profiles.recovery_reviewing = false;
                            }
                            query.error = Some(crate::shared::redact::scrub(&error));
                        }
                    }
                }
            }
            ProfileRequest::Models => {
                self.profiles.models_loading = false;
                let result = json_result(result).and_then(|value| {
                    serde_json::from_value::<ModelSelectionView>(value)
                        .map_err(|_| "Invalid model-selection metadata".into())
                });
                match result {
                    Ok(models) => {
                        self.profiles.models = Some(models);
                        self.profiles.model_error = None;
                    }
                    Err(error) => {
                        self.profiles.model_error = Some(crate::shared::redact::scrub(&error))
                    }
                }
            }
            ProfileRequest::Preflight(mutation) => {
                let isolated = self.profiles.isolated.clone();
                let context = self
                    .profile_context(&mutation.scope)
                    .filter(|current| mutation.scope == "user" || current == &mutation.origin)
                    .map(|_| mutation.origin.clone());
                let models = self.profiles.models.clone();
                let generation = self.profiles.generation;
                let owned = self.owned_runtime_pids();
                cx.spawn(async move |this, cx| {
                    let (mutation, result) = cx
                        .background_spawn(async move {
                            let result = if isolated.is_none()
                                && crate::shared::desktop_lock::is_desktop_running_except(&owned)
                            {
                                Err("Close ZCode Desktop before local management".into())
                            } else {
                                json_result(result)
                            }
                            .and_then(|value| {
                                serde_json::from_value::<AgentsListResult>(value)
                                    .map_err(|_| "Invalid Subagents preflight inventory".into())
                            })
                            .and_then(|snapshot| {
                                crate::app::profile_preflight::prepare_mutation_mode(
                                    &mutation,
                                    &snapshot,
                                    isolated.as_deref(),
                                    context.ok_or("Profile scope is unavailable")?,
                                    models.as_ref(),
                                )
                            });
                            (mutation, result)
                        })
                        .await;
                    let _ = this.update(cx, |state, cx| {
                        if state.profiles.generation != generation {
                            return;
                        }
                        let mut mutation = mutation;
                        match result {
                            Ok(params) => {
                                mutation.params = params;
                                state.enqueue_profile_request(ProfileRequest::Commit(mutation), cx);
                            }
                            Err(error) => {
                                if error == "Close ZCode Desktop before local management" {
                                    state.profiles.closing = true;
                                }
                                state.finish_profile_mutation(mutation, Err(error), cx);
                            }
                        }
                        state.maintain_profiles(cx);
                        cx.notify();
                    });
                })
                .detach();
            }
            ProfileRequest::Commit(mutation) => {
                let result = result.map(|_| ());
                if result.is_err() {
                    // 现有服务可能在改文件后才报错，不能把 RPC 失败当成未提交或自动重放。
                    self.profiles.uncertain = true;
                    self.profiles.recovery = Some(mutation.clone());
                    self.profiles.recovery_observed = false;
                    self.profiles.recovery_reviewed = false;
                    self.profiles.recovery_reviewing = false;
                }
                self.finish_profile_mutation(mutation, result, cx);
            }
        }
        self.maintain_profiles(cx);
        cx.notify();
    }

    fn finish_profile_mutation(
        &mut self,
        mutation: ProfileMutation,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.profiles.mutation_pending = false;
        let committed = result.is_ok();
        self.profiles
            .outcome_order
            .push_back(mutation.receipt.clone());
        self.profiles.outcomes.insert(
            mutation.receipt,
            result.map_err(|e| crate::shared::redact::scrub(&e)),
        );
        while self.profiles.outcome_order.len() > 64 {
            if let Some(receipt) = self.profiles.outcome_order.pop_front() {
                self.profiles.outcomes.remove(&receipt);
            }
        }
        if committed {
            if let Some(query) = self.profiles.queries.get_mut(&mutation.scope) {
                query.serial += 1;
                query.loading = false;
            }
            self.read_profiles(&mutation.scope, true, cx);
        }
    }
}

fn json_result(result: Result<ServiceValue, String>) -> Result<Value, String> {
    result.and_then(ServiceValue::into_json)
}
