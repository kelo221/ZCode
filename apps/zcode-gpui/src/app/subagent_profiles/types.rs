use super::ModelSelection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentScope {
    BuiltIn,
    Workspace,
    User,
}

impl AgentScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::Workspace => "workspace",
            Self::User => "user",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentSource {
    BuiltIn,
    User,
    Plugin,
}

impl AgentSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::User => "user",
            Self::Plugin => "plugin",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentColor {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
    Pink,
    Cyan,
}

impl AgentColor {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Purple => "purple",
            Self::Orange => "orange",
            Self::Pink => "pink",
            Self::Cyan => "cyan",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentPermissionMode {
    Auto,
    Plan,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubAgentConfig {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) system_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) color: Option<AgentColor>,
    #[serde(
        default,
        deserialize_with = "super::models::optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) model_selection: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) disallowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) inject_agents_md: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) permission_mode: Option<AgentPermissionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_turns: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mcp_servers: Option<Vec<Value>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSummary {
    pub(crate) id: String,
    #[serde(flatten)]
    pub(crate) config: SubAgentConfig,
    pub(crate) path: String,
    pub(crate) scope: AgentScope,
    pub(crate) source: AgentSource,
    pub(crate) enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plugin_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "super::models::optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) default_model_selection: Option<ModelSelection>,
    #[serde(
        default,
        deserialize_with = "super::models::optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) model_selection_override: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<Vec<AgentDiagnostic>>,
}

impl AgentSummary {
    pub(crate) fn can_edit(&self) -> bool {
        self.source == AgentSource::User
            && self.read_only != Some(true)
            && matches!(self.scope, AgentScope::User | AgentScope::Workspace)
    }
    pub(crate) fn can_delete(&self) -> bool {
        self.can_edit()
    }
    pub(crate) fn can_toggle_enabled(&self) -> bool {
        self.can_edit() && self.scope == AgentScope::User
    }
    pub(crate) fn is_builtin(&self) -> bool {
        self.scope == AgentScope::BuiltIn || self.source == AgentSource::BuiltIn
    }
    pub(crate) fn override_target(&self) -> Option<OverrideTarget> {
        if self.is_builtin() {
            match self.config.name.as_str() {
                "general-purpose" => {
                    return Some(OverrideTarget::BuiltIn(BuiltInSubagentName::GeneralPurpose));
                }
                "Explore" => return Some(OverrideTarget::BuiltIn(BuiltInSubagentName::Explore)),
                _ => {}
            }
        }
        (self.source == AgentSource::Plugin).then(|| OverrideTarget::Plugin(self.id.clone()))
    }
    pub(crate) fn delete_params(&self) -> Option<AgentDeleteParams> {
        self.can_delete().then(|| AgentDeleteParams {
            agent_id: self.id.clone(),
            file_path: self.path.clone(),
        })
    }
    pub(crate) fn enabled_params(&self, enabled: bool) -> Option<AgentEnabledParams> {
        self.can_toggle_enabled().then(|| AgentEnabledParams {
            agent_id: self.id.clone(),
            enabled,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentDiagnostic {
    pub(crate) code: String,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum UserScopeReason {
    #[serde(rename = "desktop_only")]
    DesktopOnly,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsCapability {
    pub(crate) user_scope_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) user_scope_reason: Option<UserScopeReason>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentsListResult {
    pub(crate) agents: Vec<AgentSummary>,
    pub(crate) user_agents: Vec<AgentSummary>,
    pub(crate) plugin_agents: Vec<AgentSummary>,
    pub(crate) capability: AgentsCapability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) diagnostics: Option<Vec<AgentDiagnostic>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EditableScope {
    User,
    Workspace,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum AgentProvider {
    #[serde(rename = "glm")]
    Glm,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentCreateParams {
    pub(crate) config: SubAgentConfig,
    pub(crate) provider: AgentProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<EditableScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUpdateParams {
    pub(crate) agent_id: String,
    pub(crate) config: SubAgentConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_file_path: Option<String>,
    pub(crate) provider: AgentProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<EditableScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_identity: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentDeleteParams {
    pub(crate) agent_id: String,
    pub(crate) file_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentEnabledParams {
    pub(crate) agent_id: String,
    pub(crate) enabled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) enum BuiltInSubagentName {
    #[serde(rename = "general-purpose")]
    GeneralPurpose,
    Explore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OverrideTarget {
    BuiltIn(BuiltInSubagentName),
    Plugin(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuiltInSubagentModelOverrideParams {
    pub(crate) agent_name: BuiltInSubagentName,
    #[serde(
        default,
        deserialize_with = "super::models::optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) model_selection: Option<ModelSelection>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginSubagentModelOverrideParams {
    pub(crate) agent_id: String,
    #[serde(
        default,
        deserialize_with = "super::models::optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) model_selection: Option<ModelSelection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OverridePayload {
    BuiltIn(BuiltInSubagentModelOverrideParams),
    Plugin(PluginSubagentModelOverrideParams),
}
