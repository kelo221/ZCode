use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolsMode {
    All,
    Custom,
}

// Rust trim 与 ECMAScript trim 的 BOM/NEL 边界不同；表单校验必须遵循当前 TS 后端。
pub(super) fn backend_trim(value: &str) -> &str {
    value.trim_matches(|character| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}'
        | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}'
        | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}

pub(crate) fn allows_all_tools(tools: Option<&[String]>) -> bool {
    tools.is_none_or(|tools| tools.is_empty() || tools.iter().any(|tool| tool.trim() == "*"))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FormDraft {
    pub(crate) fields: SubAgentConfig,
    pub(crate) tools_mode: ToolsMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormError {
    NameLength,
    NameCharacters,
    ReservedName,
    DescriptionRequired,
    PromptRequired,
    EmptyCustomTools,
    NotEditable,
    WorkspaceRequired,
    UnsupportedOverride,
}

impl std::fmt::Display for FormError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NameLength => "Name must be between 3 and 50 characters",
            Self::NameCharacters => "Name can only contain letters, numbers, and hyphens",
            Self::ReservedName => "Name is reserved by a built-in agent",
            Self::DescriptionRequired => "Description is required",
            Self::PromptRequired => "System prompt is required",
            Self::EmptyCustomTools => {
                "Choose at least one custom tool; an empty list means All at runtime"
            }
            Self::NotEditable => "Profile is read-only",
            Self::WorkspaceRequired => "Workspace path is required for workspace subagents",
            Self::UnsupportedOverride => "Profile does not support model overrides",
        })
    }
}

impl FormDraft {
    pub(crate) fn new() -> Self {
        Self {
            fields: SubAgentConfig {
                color: Some(AgentColor::Yellow),
                inject_agents_md: Some(true),
                ..SubAgentConfig::default()
            },
            tools_mode: ToolsMode::All,
        }
    }
    pub(crate) fn edit(agent: &AgentSummary) -> Result<Self, FormError> {
        if !agent.can_edit() {
            return Err(FormError::NotEditable);
        }
        Ok(Self {
            fields: agent.config.clone(),
            tools_mode: if allows_all_tools(agent.config.tools.as_deref()) {
                ToolsMode::All
            } else {
                ToolsMode::Custom
            },
        })
    }
    pub(crate) fn set_tools_mode(&mut self, mode: ToolsMode) {
        if mode != self.tools_mode && mode == ToolsMode::All {
            self.fields.tools = None;
        }
        self.tools_mode = mode;
    }
    pub(crate) fn config(&self) -> Result<SubAgentConfig, FormError> {
        let name = backend_trim(&self.fields.name);
        // 服务端接受大小写字母、数字和连字符；保留大小写敏感的内置名检查，不额外限制首字符。
        let length = name.encode_utf16().count();
        if !(3..=50).contains(&length) {
            return Err(FormError::NameLength);
        }
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(FormError::NameCharacters);
        }
        if matches!(name, "general-purpose" | "Explore") {
            return Err(FormError::ReservedName);
        }
        if backend_trim(&self.fields.description).is_empty() {
            return Err(FormError::DescriptionRequired);
        }
        if backend_trim(&self.fields.system_prompt).is_empty() {
            return Err(FormError::PromptRequired);
        }
        if self.tools_mode == ToolsMode::Custom
            && self
                .fields
                .tools
                .as_deref()
                .is_none_or(|tools| !tools.iter().any(|tool| !tool.trim().is_empty()))
        {
            return Err(FormError::EmptyCustomTools);
        }
        let mut config = self.fields.clone();
        config.name = name.into();
        config.description = backend_trim(&config.description).into();
        config.system_prompt = backend_trim(&config.system_prompt).into();
        // All 模式不能把旧 Custom 列表写回；已是 All 的原始缺省/空/* 表达则原样保留。
        if self.tools_mode == ToolsMode::All && !allows_all_tools(config.tools.as_deref()) {
            config.tools = None;
        }
        Ok(config)
    }
    pub(crate) fn clear_model(&mut self) {
        self.fields.model_selection = None;
    }
    pub(crate) fn select_model(
        &mut self,
        view: &ModelSelectionView,
        provider: &str,
        model: &str,
    ) -> Result<(), SelectionError> {
        models::select_model(&mut self.fields.model_selection, view, provider, model)
    }
    pub(crate) fn set_reasoning(
        &mut self,
        view: &ModelSelectionView,
        reasoning: Option<&str>,
    ) -> Result<(), SelectionError> {
        models::set_reasoning(&mut self.fields.model_selection, view, reasoning)
    }
    pub(crate) fn create_params(
        &self,
        scope: EditableScope,
        context: WorkspaceContext,
    ) -> Result<AgentCreateParams, FormError> {
        validate_workspace(scope, &context)?;
        Ok(AgentCreateParams {
            config: self.config()?,
            provider: AgentProvider::Glm,
            scope: Some(scope),
            workspace_path: context.workspace_path,
            workspace_identity: context.workspace_identity,
        })
    }
    pub(crate) fn update_params(
        &self,
        agent: &AgentSummary,
        mut context: WorkspaceContext,
    ) -> Result<AgentUpdateParams, FormError> {
        if !agent.can_edit() {
            return Err(FormError::NotEditable);
        }
        let scope = if agent.scope == AgentScope::Workspace {
            EditableScope::Workspace
        } else {
            EditableScope::User
        };
        if scope == EditableScope::Workspace && agent.project_path.is_some() {
            context.workspace_path = agent.project_path.clone();
        }
        validate_workspace(scope, &context)?;
        Ok(AgentUpdateParams {
            agent_id: agent.id.clone(),
            config: self.config()?,
            old_file_path: Some(agent.path.clone()),
            provider: AgentProvider::Glm,
            scope: Some(scope),
            workspace_path: context.workspace_path,
            workspace_identity: context.workspace_identity,
        })
    }
}

fn validate_workspace(scope: EditableScope, context: &WorkspaceContext) -> Result<(), FormError> {
    if scope == EditableScope::Workspace
        && context
            .workspace_path
            .as_deref()
            .is_none_or(|path| path.trim().is_empty())
    {
        return Err(FormError::WorkspaceRequired);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OverrideDraft {
    target: OverrideTarget,
    pub(crate) selection: Option<ModelSelection>,
}

impl OverrideDraft {
    pub(crate) fn from_agent(agent: &AgentSummary) -> Result<Self, FormError> {
        Ok(Self {
            target: agent
                .override_target()
                .ok_or(FormError::UnsupportedOverride)?,
            selection: agent.model_selection_override.clone(),
        })
    }
    pub(crate) fn clear(&mut self) {
        self.selection = None;
    }
    pub(crate) fn select_model(
        &mut self,
        view: &ModelSelectionView,
        provider: &str,
        model: &str,
    ) -> Result<(), SelectionError> {
        models::select_model(&mut self.selection, view, provider, model)
    }
    pub(crate) fn set_reasoning(
        &mut self,
        view: &ModelSelectionView,
        reasoning: Option<&str>,
    ) -> Result<(), SelectionError> {
        models::set_reasoning(&mut self.selection, view, reasoning)
    }
    pub(crate) fn payload(&self) -> OverridePayload {
        // 清除覆盖必须省略整份 modelSelection，不能留下 provider/model 或旧 reasoning。
        match &self.target {
            OverrideTarget::BuiltIn(name) => {
                OverridePayload::BuiltIn(BuiltInSubagentModelOverrideParams {
                    agent_name: *name,
                    model_selection: self.selection.clone(),
                })
            }
            OverrideTarget::Plugin(id) => {
                OverridePayload::Plugin(PluginSubagentModelOverrideParams {
                    agent_id: id.clone(),
                    model_selection: self.selection.clone(),
                })
            }
        }
    }
}
