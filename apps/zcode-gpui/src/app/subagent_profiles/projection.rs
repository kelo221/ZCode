use super::*;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceContext {
    pub(crate) workspace_path: Option<String>,
    pub(crate) workspace_identity: Option<String>,
}

impl WorkspaceContext {
    pub(crate) fn identity_key(&self) -> &str {
        self.workspace_identity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(self.workspace_path.as_deref())
            .unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsScope {
    User,
    Workspace(WorkspaceContext),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProfileKey {
    pub(crate) scope: AgentScope,
    pub(crate) workspace: String,
    pub(crate) id: String,
}

impl ProfileKey {
    pub(crate) fn new(agent: &AgentSummary, context: &WorkspaceContext) -> Self {
        let workspace = if agent.scope != AgentScope::Workspace {
            ""
        } else if agent
            .project_path
            .as_deref()
            .is_some_and(|path| Some(path) != context.workspace_path.as_deref())
        {
            agent.project_path.as_deref().unwrap_or("")
        } else {
            context.identity_key()
        };
        Self {
            scope: agent.scope,
            workspace: workspace.into(),
            id: agent.id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProfileRow {
    pub(crate) key: ProfileKey,
    pub(crate) agent: AgentSummary,
}

impl AgentsListResult {
    pub(crate) fn settings_rows(&self, context: &WorkspaceContext) -> Vec<ProfileRow> {
        let mut seen = HashSet::new();
        // agents 是 runtime 投影；插件裸名 alias 与规范名同 id，设置只用 pluginAgents 资源投影。
        self.agents
            .iter()
            .filter(|agent| agent.source != AgentSource::Plugin)
            .chain(self.plugin_agents.iter())
            .filter_map(|agent| {
                let key = ProfileKey::new(agent, context);
                seen.insert(key.clone()).then(|| ProfileRow {
                    key,
                    agent: agent.clone(),
                })
            })
            .collect()
    }
    pub(crate) fn visible_rows(&self, scope: &SettingsScope, query: &str) -> Vec<ProfileRow> {
        let empty = WorkspaceContext::default();
        let context = match scope {
            SettingsScope::User => &empty,
            SettingsScope::Workspace(context) => context,
        };
        let query = query.trim().to_lowercase();
        self.settings_rows(context)
            .into_iter()
            .filter(|row| matches_scope(&row.agent, scope) && matches_query(&row.agent, &query))
            .collect()
    }
}

pub(crate) fn matches_scope(agent: &AgentSummary, scope: &SettingsScope) -> bool {
    match scope {
        SettingsScope::User => agent.scope != AgentScope::Workspace,
        SettingsScope::Workspace(context) => {
            if agent.is_builtin() || agent.scope != AgentScope::Workspace {
                return false;
            }
            if agent.source == AgentSource::Plugin {
                return agent.project_path.is_some()
                    && agent.project_path == context.workspace_path;
            }
            agent
                .project_path
                .as_deref()
                .filter(|path| !path.is_empty())
                .is_none_or(|path| Some(path) == context.workspace_path.as_deref())
        }
    }
}

pub(crate) fn matches_query(agent: &AgentSummary, normalized_query: &str) -> bool {
    if normalized_query.is_empty() {
        return true;
    }
    let config = &agent.config;
    let model = config
        .model_selection
        .as_ref()
        .map(|selection| format!("{}/{}", selection.provider_id, selection.model_id))
        .unwrap_or_default();
    let tools = config.tools.as_deref().unwrap_or_default().join(" ");
    let disallowed = config
        .disallowed_tools
        .as_deref()
        .unwrap_or_default()
        .join(" ");
    let skills = config.skills.as_deref().unwrap_or_default().join(" ");
    [
        config.name.as_str(),
        config.description.as_str(),
        model.as_str(),
        agent.path.as_str(),
        agent.scope.as_str(),
        agent.source.as_str(),
        tools.as_str(),
        disallowed.as_str(),
        skills.as_str(),
    ]
    .join(" ")
    .to_lowercase()
    .contains(normalized_query)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AgentGroups {
    pub(crate) built_in: Vec<ProfileRow>,
    pub(crate) plugin: Vec<ProfileRow>,
    pub(crate) user: Vec<ProfileRow>,
}

pub(crate) fn group_rows(rows: &[ProfileRow]) -> AgentGroups {
    let mut groups = AgentGroups::default();
    for row in rows {
        let target = if row.agent.can_edit() {
            &mut groups.user
        } else if row.agent.source == AgentSource::Plugin {
            &mut groups.plugin
        } else {
            &mut groups.built_in
        };
        target.push(row.clone());
    }
    groups
}

pub(crate) fn group_plugin_rows(rows: &[ProfileRow]) -> Vec<(String, Vec<ProfileRow>)> {
    let mut groups: BTreeMap<String, Vec<ProfileRow>> = BTreeMap::new();
    for row in rows {
        let agent = &row.agent;
        let key = agent
            .plugin_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .or_else(|| {
                agent
                    .plugin_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
            })
            .or_else(|| {
                agent
                    .config
                    .name
                    .split(':')
                    .next()
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or("Plugin");
        groups.entry(key.into()).or_default().push(row.clone());
    }
    groups.into_iter().collect()
}
