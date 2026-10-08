use crate::app::subagent_profiles::{AgentSummary, AgentsListResult, ModelSelectionView};
use crate::backend::services_rpc::ServiceClient;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

#[derive(Default)]
pub(crate) struct ProfileQuery {
    pub snapshot: Option<AgentsListResult>,
    pub loading: bool,
    pub serial: u64,
    pub error: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ProfileMutation {
    pub receipt: String,
    pub scope: String,
    pub method: String,
    pub params: serde_json::Value,
    pub baseline: Option<AgentSummary>,
    pub origin: crate::app::subagent_profiles::WorkspaceContext,
}

pub(crate) enum ProfileRequest {
    List { scope: String, serial: u64 },
    Models,
    Preflight(ProfileMutation),
    Commit(ProfileMutation),
}

#[derive(Default)]
pub(crate) struct ProfileState {
    pub isolated: Option<std::sync::Arc<crate::shared::isolation::IsolatedSettings>>,
    pub local_enabled: bool,
    pub activating: bool,
    pub closing: bool,
    pub stopping: bool,
    pub recovery: Option<ProfileMutation>,
    pub recovery_observed: bool,
    pub recovery_reviewing: bool,
    pub recovery_reviewed: bool,
    pub review_after_close: Option<String>,
    pub client: Option<ServiceClient>,
    pub startup_exit: Option<crate::backend::services_rpc::ServiceExitObserver>,
    pub starting: bool,
    pub generation: u64,
    pub queries: HashMap<String, ProfileQuery>,
    pub models: Option<ModelSelectionView>,
    pub model_error: Option<String>,
    pub models_loading: bool,
    pub queue: VecDeque<ProfileRequest>,
    pub mutation_pending: bool,
    pub uncertain: bool,
    pub outcomes: HashMap<String, Result<(), String>>,
    pub outcome_order: VecDeque<String>,
    pub connection_error: Option<String>,
    pub last_activity: Option<Instant>,
}

impl ProfileState {
    pub(crate) fn ready(&self) -> bool {
        (self.isolated.is_some() || self.local_enabled)
            && !self.activating
            && !self.closing
            && !self.stopping
    }
    pub(crate) fn idle(&self) -> bool {
        !self.starting
            && !self.mutation_pending
            && self.queue.is_empty()
            && !self.models_loading
            && !self.queries.values().any(|query| query.loading)
            && self
                .client
                .as_ref()
                .is_none_or(|client| client.pending_count() == 0)
    }
    pub(crate) fn idle_expired(&self) -> bool {
        self.client.is_some()
            && self.idle()
            && self
                .last_activity
                .is_some_and(|t| t.elapsed() >= std::time::Duration::from_secs(60))
    }

    pub(crate) fn shutdown(&mut self) {
        self.generation += 1;
        self.starting = false;
        if let Some(mut client) = self.client.take() {
            client.shutdown();
        }
        self.queue.clear();
    }
}
