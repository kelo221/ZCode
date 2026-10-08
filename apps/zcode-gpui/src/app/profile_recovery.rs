use super::profile_state::ProfileMutation;
use super::subagent_profiles::{AgentSource, AgentsListResult};

pub(crate) fn observed(m: &ProfileMutation, snapshot: &AgentsListResult) -> bool {
    let rows: Vec<_> = snapshot
        .agents
        .iter()
        .filter(|a| a.source != AgentSource::Plugin)
        .chain(snapshot.plugin_agents.iter())
        .collect();
    match m.method.as_str() {
        "createAgent" | "updateAgent" => {
            let Some(config) = m.params.get("config").cloned().and_then(|c| {
                serde_json::from_value::<super::subagent_profiles::SubAgentConfig>(c).ok()
            }) else {
                return false;
            };
            let expected_scope = if m.scope == "user" {
                "user"
            } else {
                "workspace"
            };
            let matches: Vec<_> = rows
                .iter()
                .filter(|a| {
                    a.config.name == config.name
                        && a.scope.as_str() == expected_scope
                        && a.source == AgentSource::User
                        && (a.scope != super::subagent_profiles::AgentScope::Workspace
                            || a.project_path == m.origin.workspace_path)
                        && a.config == config
                })
                .collect();
            matches.len() == 1
                && m.baseline.as_ref().is_none_or(|old| {
                    old.config.name == config.name
                        || !rows.iter().any(|a| a.id == old.id && a.path == old.path)
                })
        }
        "deleteAgent" => m
            .baseline
            .as_ref()
            .is_some_and(|old| !rows.iter().any(|a| a.id == old.id && a.path == old.path)),
        "setEnabled" => m.baseline.as_ref().is_some_and(|old| {
            rows.iter().any(|a| {
                a.id == old.id
                    && a.path == old.path
                    && Some(a.enabled) == m.params.get("enabled").and_then(|v| v.as_bool())
            })
        }),
        "setBuiltInModelOverride" | "setPluginAgentModelOverride" => {
            let selection = m
                .params
                .get("modelSelection")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok());
            m.baseline.as_ref().is_some_and(|old| {
                rows.iter().any(|a| {
                    a.id == old.id
                        && a.scope == old.scope
                        && a.source == old.source
                        && a.project_path == old.project_path
                        && a.model_selection_override == selection
                })
            })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn unique_supported_create_is_observed_but_duplicate_or_mismatch_is_not() {
        let m = ProfileMutation {
            origin: Default::default(),
            receipt: "r".into(),
            scope: "user".into(),
            method: "createAgent".into(),
            params: json!({"config":{"name":"a","description":"v2","systemPrompt":"p"}}),
            baseline: None,
        };
        let row = json!({"id":"user:a","name":"a","description":"v2","systemPrompt":"p","path":"C:/scratch/a.md","scope":"user","source":"user","enabled":true,"readOnly":false});
        let make = |rows| {
            serde_json::from_value::<AgentsListResult>(json!({"agents":rows,"userAgents":[],"pluginAgents":[],"capability":{"userScopeAvailable":true}})).unwrap()
        };
        assert!(observed(&m, &make(vec![row.clone()])));
        assert!(!observed(&m, &make(vec![row.clone(), row])));
        assert!(!observed(&m, &make(vec![])));
    }
}
