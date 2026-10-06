use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use std::collections::HashMap;

pub(crate) struct QueryState<T> {
    pub value: Option<T>,
    pub loading: bool,
    pub attempted: bool,
    pub error: Option<String>,
    pub revision: u64,
}
impl<T> Default for QueryState<T> {
    fn default() -> Self {
        Self {
            value: None,
            loading: false,
            attempted: false,
            error: None,
            revision: 0,
        }
    }
}
impl<T> QueryState<T> {
    pub(crate) fn start(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.loading = true;
        self.attempted = true;
        self.error = None;
    }
    pub(crate) fn finish(&mut self, value: T) {
        self.value = Some(value);
        self.loading = false;
        self.error = None;
    }
    pub(crate) fn fail(&mut self, error: &str) {
        self.loading = false;
        self.attempted = true;
        self.error = Some(crate::shared::redact::scrub(error));
    }
}

pub(crate) struct WorkspaceInspection {
    pub mcp: QueryState<Vec<crate::shared::mcp::McpServerSnapshot>>,
    pub plugins: QueryState<crate::shared::plugins::PluginsOverviewResult>,
    pub usage: HashMap<String, QueryState<crate::shared::usage_stats::AppUsageSnapshot>>,
    pub usage_range: String,
    pub plugin_operation_error: Option<String>,
    pub plugin_detail: Option<crate::backend::plugin_detail::PluginDetailIdentity>,
    pub plugin_config: Option<crate::backend::plugin_config::PluginConfigDraft>,
    pub plugin_descriptions:
        HashMap<String, QueryState<crate::shared::plugin_description::PluginDescription>>,
}
impl Default for WorkspaceInspection {
    fn default() -> Self {
        Self {
            mcp: Default::default(),
            plugins: Default::default(),
            usage: Default::default(),
            usage_range: "7d".into(),
            plugin_operation_error: None,
            plugin_detail: None,
            plugin_config: None,
            plugin_descriptions: HashMap::new(),
        }
    }
}

impl AppState {
    pub(crate) fn plugin_action_pending(&self, workspace: &str) -> bool {
        self.ws(workspace).is_some_and(|ws| {
            ws.pending
                .values()
                .any(|p| matches!(p, Pending::PluginAction(_)))
        })
    }
    pub(crate) fn active_inspection(&self) -> Option<&WorkspaceInspection> {
        self.active_ws_key()
            .and_then(|key| self.ws(&key))
            .map(|ws| &ws.inspection)
    }
    pub(crate) fn settle_inspection_error(
        &mut self,
        workspace: &str,
        pending: &Pending,
        error: &str,
    ) {
        if let Some(ws) = self.ws_mut(workspace) {
            match pending {
                Pending::FetchMcpList => ws.inspection.mcp.fail(error),
                Pending::FetchPluginsOverview => ws.inspection.plugins.fail(error),
                Pending::PluginAction(_) => {
                    ws.inspection.plugin_operation_error =
                        Some(crate::shared::redact::scrub(error));
                }
                Pending::FetchUsageStats(range) => ws
                    .inspection
                    .usage
                    .entry(range.clone())
                    .or_default()
                    .fail(error),
                _ => {}
            }
        }
    }
    pub(crate) fn active_usage(&self) -> Option<&crate::shared::usage_stats::AppUsageSnapshot> {
        let inspection = self.active_inspection()?;
        inspection
            .usage
            .get(&inspection.usage_range)?
            .value
            .as_ref()
    }
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
