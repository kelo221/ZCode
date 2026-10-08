use super::*;
use serde_json::{Value, json};

fn agent() -> AgentSummary {
    serde_json::from_value(json!({"id":"user:workspace:synthetic","name":"Synthetic",
        "description":"Description","systemPrompt":"Prompt","scope":"workspace","source":"user",
        "path":"fixture/project/synthetic.md","projectPath":"fixture/project","enabled":true,
        "color":"pink","modelSelection":{"providerId":"fixture","modelId":"old",
            "options":{"reasoningLevel":"old-high"}},"tools":["Read","FixtureTool"],
        "disallowedTools":["FixtureBlocked"],"skills":["FixtureSkill"],"injectAgentsMd":false,
        "permissionMode":"plan","maxTurns":0,"background":false,
        "mcpServers":[{"name":"fixture","transport":{"type":"fixture"}}]}))
    .unwrap()
}

fn draft() -> FormDraft {
    let mut draft = FormDraft::new();
    draft.fields.name = "Synthetic".into();
    draft.fields.description = "Description".into();
    draft.fields.system_prompt = "Prompt".into();
    draft
}

#[test]
fn edit_clones_every_supported_field_and_preserves_untouched_data() {
    let original = agent();
    let mut draft = FormDraft::edit(&original).unwrap();
    assert_eq!(draft.config().unwrap(), original.config);
    draft.fields.description = "Updated".into();
    let mut expected = original.config.clone();
    expected.description = "Updated".into();
    assert_eq!(draft.config().unwrap(), expected);
    assert_eq!(original.config.description, "Description");
    draft
        .fields
        .tools
        .as_mut()
        .unwrap()
        .push("AnotherFixtureTool".into());
    assert_eq!(original.config.tools.unwrap().len(), 2);
}

#[test]
fn backend_name_semantics_required_fields_and_exact_reserved_case() {
    let mut draft = draft();
    for name in [
        "---",
        "123",
        "Explorex",
        "explore",
        "GENERAL-PURPOSE",
        "  AbC-123  ",
    ] {
        draft.fields.name = name.into();
        assert!(draft.config().is_ok(), "{name}");
    }
    for name in ["ab", "a_b", "a b", "中文名", "Explore", "general-purpose"] {
        draft.fields.name = name.into();
        assert!(draft.config().is_err(), "{name}");
    }
    draft.fields.name = "\u{feff}Synthetic\u{feff}".into();
    assert_eq!(draft.config().unwrap().name, "Synthetic");
    draft.fields.name = "\u{0085}Synthetic\u{0085}".into();
    assert_eq!(draft.config(), Err(FormError::NameCharacters));
    draft.fields.name = "a".repeat(50);
    assert!(draft.config().is_ok());
    draft.fields.name.push('a');
    assert!(draft.config().is_err());
    draft.fields.name = "synthetic".into();
    draft.fields.description = " \n ".into();
    assert_eq!(draft.config(), Err(FormError::DescriptionRequired));
    draft.fields.description = " description ".into();
    draft.fields.system_prompt = "\t".into();
    assert_eq!(draft.config(), Err(FormError::PromptRequired));
    draft.fields.system_prompt = " prompt ".into();
    assert_eq!(draft.config().unwrap().description, "description");
    assert_eq!(draft.config().unwrap().system_prompt, "prompt");
}

#[test]
fn all_tools_are_not_none_and_explicit_empty_custom_is_invalid() {
    for tools in [None, Some(vec![]), Some(vec![" * ".into()])] {
        let mut agent = agent();
        agent.config.tools = tools.clone();
        let draft = FormDraft::edit(&agent).unwrap();
        assert_eq!(draft.tools_mode, ToolsMode::All);
        assert_eq!(draft.config().unwrap().tools, tools);
    }
    let mut draft = draft();
    draft.tools_mode = ToolsMode::Custom;
    draft.fields.tools = Some(vec![]);
    assert_eq!(draft.config(), Err(FormError::EmptyCustomTools));
    draft.fields.tools = Some(vec!["".into(), "  ".into()]);
    assert_eq!(draft.config(), Err(FormError::EmptyCustomTools));
    draft.fields.tools = Some(vec!["FixtureTool".into()]);
    assert!(draft.config().is_ok());
    draft.tools_mode = ToolsMode::All;
    assert_eq!(draft.config().unwrap().tools, None);
    draft.tools_mode = ToolsMode::Custom;
    draft.set_tools_mode(ToolsMode::All);
    assert_eq!(draft.config().unwrap().tools, None);
}

#[test]
fn create_update_payloads_use_existing_public_shape_without_summary_metadata() {
    let draft = draft();
    let context = WorkspaceContext {
        workspace_path: Some("fixture/project".into()),
        workspace_identity: Some("fixture-identity".into()),
    };
    let create = draft
        .create_params(EditableScope::Workspace, context.clone())
        .unwrap();
    let raw = serde_json::to_value(create).unwrap();
    assert_eq!(raw["provider"], "glm");
    assert_eq!(raw["scope"], "workspace");
    assert_eq!(raw["workspacePath"], "fixture/project");
    assert_eq!(raw["workspaceIdentity"], "fixture-identity");
    assert!(raw.get("agentId").is_none());
    let update = draft.update_params(&agent(), context).unwrap();
    let raw = serde_json::to_value(update).unwrap();
    assert_eq!(raw["agentId"], "user:workspace:synthetic");
    assert_eq!(raw["oldFilePath"], "fixture/project/synthetic.md");
    assert!(raw["config"].get("path").is_none());
    assert!(raw["config"].get("enabled").is_none());
    assert!(
        draft
            .create_params(EditableScope::Workspace, WorkspaceContext::default())
            .is_err()
    );
    let user = serde_json::to_value(
        draft
            .create_params(EditableScope::User, WorkspaceContext::default())
            .unwrap(),
    )
    .unwrap();
    assert!(user.get("workspacePath").is_none());
    assert!(user.get("workspaceIdentity").is_none());
}

#[test]
fn edit_delete_update_and_enable_payloads_reject_readonly_and_wrong_scope() {
    let mut agent = agent();
    assert!(agent.delete_params().is_some());
    assert!(agent.enabled_params(false).is_none());
    agent.scope = AgentScope::User;
    assert!(agent.enabled_params(false).is_some());
    agent.read_only = Some(true);
    assert_eq!(FormDraft::edit(&agent), Err(FormError::NotEditable));
    assert!(
        draft()
            .update_params(&agent, WorkspaceContext::default())
            .is_err()
    );
    assert!(agent.delete_params().is_none());
    assert!(agent.enabled_params(false).is_none());
}

#[test]
fn override_clear_omits_full_selection_and_does_not_replace_declared_default() {
    let mut raw = serde_json::to_value(agent()).unwrap();
    raw["source"] = json!("plugin");
    raw["id"] = json!("plugin:fixture@store:synthetic");
    raw["defaultModelSelection"] = json!({"providerId":"fixture","modelId":"declared"});
    raw["modelSelectionOverride"] = raw["modelSelection"].clone();
    let plugin: AgentSummary = serde_json::from_value(raw).unwrap();
    let mut draft = OverrideDraft::from_agent(&plugin).unwrap();
    assert_eq!(draft.selection, plugin.model_selection_override);
    draft.clear();
    let OverridePayload::Plugin(payload) = draft.payload() else {
        panic!("plugin payload")
    };
    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        json!({"agentId":"plugin:fixture@store:synthetic"})
    );
    assert_eq!(plugin.default_model_selection.unwrap().model_id, "declared");
    let mut raw = serde_json::to_value(agent()).unwrap();
    raw["scope"] = json!("built-in");
    raw["source"] = json!("built-in");
    raw["name"] = json!("Explore");
    let builtin: AgentSummary = serde_json::from_value(raw).unwrap();
    let mut draft = OverrideDraft::from_agent(&builtin).unwrap();
    draft.clear();
    let OverridePayload::BuiltIn(payload) = draft.payload() else {
        panic!("builtin payload")
    };
    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        json!({"agentName":"Explore"})
    );
}

#[test]
fn unknown_summary_metadata_is_not_promised_in_config_payloads() {
    let mut raw: Value = serde_json::to_value(agent()).unwrap();
    raw["futureMetadata"] = json!({"synthetic":true});
    let parsed: AgentSummary = serde_json::from_value(raw).unwrap();
    let config = FormDraft::edit(&parsed).unwrap().config().unwrap();
    assert!(
        serde_json::to_value(config)
            .unwrap()
            .get("futureMetadata")
            .is_none()
    );
}
