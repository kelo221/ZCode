use crate::app::profile_preflight::{prepare_mutation, prepare_mutation_mode};
use crate::app::profile_state::ProfileMutation;
use crate::app::subagent_profiles::{AgentSummary, AgentsListResult, WorkspaceContext};
use crate::shared::isolation::IsolatedSettings;
use serde_json::json;

#[test]
fn local_mode_rebuilds_canonical_paths_and_rejects_another_workspace() {
    let isolated = IsolatedSettings::create().unwrap();
    let mut row = agent(&isolated);
    let context = WorkspaceContext {
        workspace_path: Some(isolated.workspace().to_string_lossy().into_owned()),
        workspace_identity: Some("registered".into()),
    };
    let mut mutation = ProfileMutation {
        origin: Default::default(),
        receipt: "r".into(),
        scope: "user".into(),
        method: "deleteAgent".into(),
        params: json!({"filePath":"untrusted"}),
        baseline: Some(row.clone()),
    };
    assert_eq!(
        prepare_mutation_mode(&mutation, &inventory(&row), None, context.clone(), None).unwrap()["filePath"],
        row.path
    );
    row.scope = crate::app::subagent_profiles::AgentScope::Workspace;
    row.project_path = Some(isolated.home().to_string_lossy().into_owned());
    mutation.scope = "registered".into();
    mutation.baseline = Some(row.clone());
    assert!(prepare_mutation_mode(&mutation, &inventory(&row), None, context, None).is_err());
}

fn inventory(agent: &AgentSummary) -> AgentsListResult {
    serde_json::from_value(json!({"agents":[agent],"userAgents":[agent],"pluginAgents":[],"capability":{"userScopeAvailable":true}})).unwrap()
}

fn agent(root: &IsolatedSettings) -> AgentSummary {
    let path = root.home().join("test.md");
    std::fs::write(&path, "scratch fixture").unwrap();
    serde_json::from_value(
        json!({"id":"user:test","name":"test-agent","description":"Test","systemPrompt":"Prompt",
        "path":path,"scope":"user","source":"user","enabled":true}),
    )
    .unwrap()
}

#[test]
fn preflight_replaces_caller_paths_with_fresh_row_and_refuses_stale_rows() {
    let isolated = IsolatedSettings::create().unwrap();
    let agent = agent(&isolated);
    let snapshot = inventory(&agent);
    let context = WorkspaceContext {
        workspace_path: Some(isolated.workspace().to_string_lossy().into_owned()),
        workspace_identity: None,
    };
    let mut mutation = ProfileMutation {
        origin: Default::default(),
        receipt: "test".into(),
        scope: "user".into(),
        method: "deleteAgent".into(),
        params: json!({"agentId":"wrong","filePath":"outside"}),
        baseline: Some(agent.clone()),
    };
    let params = prepare_mutation(&mutation, &snapshot, &isolated, context.clone(), None).unwrap();
    assert_eq!(params["filePath"], agent.path);
    assert_eq!(params["agentId"], agent.id);
    mutation.baseline.as_mut().unwrap().config.description = "stale".into();
    assert!(prepare_mutation(&mutation, &snapshot, &isolated, context, None).is_err());
}

#[test]
fn duplicate_names_are_checked_per_editable_scope_not_runtime_shadowing() {
    let isolated = IsolatedSettings::create().unwrap();
    let agent = agent(&isolated);
    let snapshot = inventory(&agent);
    let context = WorkspaceContext {
        workspace_path: Some(isolated.workspace().to_string_lossy().into_owned()),
        workspace_identity: Some("scratch-key".into()),
    };
    let mut mutation = ProfileMutation {
        origin: Default::default(),
        receipt: "test".into(),
        scope: "project".into(),
        method: "createAgent".into(),
        params: json!({"config":agent.config}),
        baseline: None,
    };
    let params = prepare_mutation(&mutation, &snapshot, &isolated, context.clone(), None).unwrap();
    assert_eq!(params["scope"], "workspace");
    assert_eq!(params["workspaceIdentity"], "scratch-key");
    mutation.scope = "user".into();
    assert!(prepare_mutation(&mutation, &snapshot, &isolated, context, None).is_err());
}

#[test]
fn override_method_and_scope_cannot_bypass_permissions() {
    let isolated = IsolatedSettings::create().unwrap();
    let mut agent = agent(&isolated);
    agent.scope = crate::app::subagent_profiles::AgentScope::Workspace;
    let context = WorkspaceContext {
        workspace_path: Some(isolated.workspace().to_string_lossy().into_owned()),
        workspace_identity: None,
    };
    let mut mutation = ProfileMutation {
        origin: Default::default(),
        receipt: "test".into(),
        scope: "project".into(),
        method: "setEnabled".into(),
        params: json!({"enabled":false}),
        baseline: Some(agent.clone()),
    };
    assert!(
        prepare_mutation(
            &mutation,
            &inventory(&agent),
            &isolated,
            context.clone(),
            None
        )
        .is_err()
    );
    agent.source = crate::app::subagent_profiles::AgentSource::Plugin;
    agent.id = "plugin:test".into();
    mutation.baseline = Some(agent.clone());
    mutation.method = "setBuiltInModelOverride".into();
    assert!(prepare_mutation(&mutation, &inventory(&agent), &isolated, context, None).is_err());
}
