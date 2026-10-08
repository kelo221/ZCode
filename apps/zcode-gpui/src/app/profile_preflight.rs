use crate::app::profile_state::ProfileMutation;
use crate::app::subagent_profiles::{
    AgentScope, AgentSource, AgentsListResult, EditableScope, FormDraft, ModelSelection,
    ModelSelectionView, OverrideDraft, OverridePayload, WorkspaceContext,
};
use crate::shared::isolation::IsolatedSettings;
use serde_json::Value;

#[cfg(test)]
pub(crate) fn prepare_mutation(
    mutation: &ProfileMutation,
    snapshot: &AgentsListResult,
    isolated: &IsolatedSettings,
    context: WorkspaceContext,
    models: Option<&ModelSelectionView>,
) -> Result<Value, String> {
    prepare_mutation_mode(mutation, snapshot, Some(isolated), context, models)
}

pub(crate) fn prepare_mutation_mode(
    mutation: &ProfileMutation,
    snapshot: &AgentsListResult,
    isolated: Option<&IsolatedSettings>,
    context: WorkspaceContext,
    models: Option<&ModelSelectionView>,
) -> Result<Value, String> {
    if !context.workspace_path.as_ref().is_some_and(|path| {
        let path = std::path::Path::new(path);
        path.is_absolute() && isolated.is_none_or(|i| i.allows_workspace(path))
    }) {
        return Err("Workspace is unavailable for this management mode".into());
    }
    let agents = snapshot
        .agents
        .iter()
        .filter(|a| a.source != AgentSource::Plugin)
        .chain(snapshot.plugin_agents.iter());
    let current = if let Some(baseline) = &mutation.baseline {
        let current = agents
            .clone()
            .find(|agent| {
                agent.id == baseline.id
                    && agent.path == baseline.path
                    && agent.scope == baseline.scope
            })
            .ok_or("Profile was removed or replaced; refresh before editing")?;
        if current != baseline {
            return Err("Profile changed after this form opened; refresh before editing".into());
        }
        if current.scope == AgentScope::Workspace
            && current.project_path.as_deref().is_some_and(|p| {
                !same_path(p, context.workspace_path.as_deref().unwrap_or_default())
            })
        {
            return Err("Profile does not belong to the originating workspace".into());
        }
        Some(current)
    } else {
        None
    };
    let selection = mutation
        .params
        .get("modelSelection")
        .cloned()
        .map(serde_json::from_value::<ModelSelection>)
        .transpose()
        .map_err(|_| "Invalid model selection")?;
    let validate_selection = |selection: &Option<ModelSelection>| -> Result<(), String> {
        if let Some(selection) = selection {
            models
                .ok_or("Model metadata is unavailable")?
                .validate_selection(selection)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    match mutation.method.as_str() {
        "createAgent" | "updateAgent" => {
            if mutation.scope == "user" && !snapshot.capability.user_scope_available {
                return Err("User profiles are unavailable".into());
            }
            let mut draft = FormDraft::new();
            draft.fields = serde_json::from_value(
                mutation
                    .params
                    .get("config")
                    .cloned()
                    .ok_or("Profile config is missing")?,
            )
            .map_err(|_| "Invalid profile config")?;
            draft.tools_mode =
                if crate::app::subagent_profiles::allows_all_tools(draft.fields.tools.as_deref()) {
                    crate::app::subagent_profiles::ToolsMode::All
                } else {
                    crate::app::subagent_profiles::ToolsMode::Custom
                };
            validate_selection(&draft.fields.model_selection)?;
            let scope = if mutation.scope == "user" {
                EditableScope::User
            } else {
                EditableScope::Workspace
            };
            let expected_scope = if scope == EditableScope::User {
                AgentScope::User
            } else {
                AgentScope::Workspace
            };
            let config = draft.config().map_err(|e| e.to_string())?;
            if agents.clone().any(|a| {
                a.source == AgentSource::User
                    && a.scope == expected_scope
                    && (a.scope != AgentScope::Workspace
                        || a.project_path.as_deref().is_none_or(|p| {
                            same_path(p, context.workspace_path.as_deref().unwrap_or_default())
                        }))
                    && a.config.name.eq_ignore_ascii_case(&config.name)
                    && current.is_none_or(|row| row.id != a.id)
            }) {
                return Err("A profile with this name already exists in this scope".into());
            }
            if mutation.method == "createAgent" {
                if current.is_some() {
                    return Err("Create cannot target an existing profile".into());
                }
                Ok(serde_json::to_value(
                    draft
                        .create_params(scope, context)
                        .map_err(|e| e.to_string())?,
                )
                .unwrap())
            } else {
                let current = current.ok_or("Update requires a current profile")?;
                if current.scope != expected_scope
                    || !current.can_edit()
                    || !std::path::Path::new(&current.path).is_absolute()
                    || isolated
                        .is_some_and(|i| !i.allows_workspace(std::path::Path::new(&current.path)))
                {
                    return Err("Profile is not editable in this scope".into());
                }
                Ok(serde_json::to_value(
                    draft
                        .update_params(current, context)
                        .map_err(|e| e.to_string())?,
                )
                .unwrap())
            }
        }
        "deleteAgent" => {
            let current = current.ok_or("Delete requires a current profile")?;
            if current.scope == AgentScope::User && !snapshot.capability.user_scope_available {
                return Err("User profiles are unavailable".into());
            }
            if !std::path::Path::new(&current.path).is_absolute()
                || isolated
                    .is_some_and(|i| !i.allows_workspace(std::path::Path::new(&current.path)))
            {
                return Err("Profile path is unavailable for this management mode".into());
            }
            Ok(
                serde_json::to_value(current.delete_params().ok_or("Profile cannot be deleted")?)
                    .unwrap(),
            )
        }
        "setEnabled" => {
            if !snapshot.capability.user_scope_available {
                return Err("User profiles are unavailable".into());
            }
            let current = current.ok_or("Enablement requires a current profile")?;
            let enabled = mutation
                .params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("Enablement must be boolean")?;
            Ok(serde_json::to_value(
                current
                    .enabled_params(enabled)
                    .ok_or("Profile cannot change enablement")?,
            )
            .unwrap())
        }
        "setBuiltInModelOverride" | "setPluginAgentModelOverride" => {
            validate_selection(&selection)?;
            let current = current.ok_or("Override requires a current profile")?;
            let mut draft = OverrideDraft::from_agent(current).map_err(|e| e.to_string())?;
            draft.selection = selection;
            match draft.payload() {
                OverridePayload::BuiltIn(params)
                    if mutation.method == "setBuiltInModelOverride" =>
                {
                    Ok(serde_json::to_value(params).unwrap())
                }
                OverridePayload::Plugin(params)
                    if mutation.method == "setPluginAgentModelOverride" =>
                {
                    Ok(serde_json::to_value(params).unwrap())
                }
                _ => Err("Override method does not match this profile".into()),
            }
        }
        _ => Err("Unsupported profile mutation".into()),
    }
}

fn same_path(a: &str, b: &str) -> bool {
    let path = |s: &str| s.strip_prefix(r"\\?\").unwrap_or(s).replace('/', r"\");
    if cfg!(windows) {
        path(a).eq_ignore_ascii_case(&path(b))
    } else {
        a == b
    }
}
