use super::*;
use serde_json::{Value, json};

fn raw_agent(id: &str, scope: &str, source: &str) -> Value {
    json!({"id":id,"name":"Same-Name","description":"Synthetic helper",
        "systemPrompt":"Synthetic prompt","path":"fixture/profile.md",
        "scope":scope,"source":source,"enabled":true})
}

fn snapshot(agents: Vec<Value>, plugins: Vec<Value>) -> AgentsListResult {
    serde_json::from_value(
        json!({"agents":agents,"userAgents":[],"pluginAgents":plugins,
        "capability":{"userScopeAvailable":true}}),
    )
    .unwrap()
}

fn workspace(path: &str, identity: Option<&str>) -> WorkspaceContext {
    WorkspaceContext {
        workspace_path: Some(path.into()),
        workspace_identity: identity.map(str::to_owned),
    }
}

#[test]
fn exact_lists_capability_diagnostics_and_supported_fields_round_trip() {
    let mut agent = raw_agent("user:user:synthetic", "user", "user");
    agent.as_object_mut().unwrap().extend(json!({
        "color":"cyan","modelSelection":{"providerId":"fixture-provider","modelId":"fixture-model",
            "options":{"reasoningLevel":"high"}},
        "defaultModelSelection":{"providerId":"fixture-provider","modelId":"default"},
        "modelSelectionOverride":{"providerId":"fixture-provider","modelId":"override"},
        "tools":["Read","FixtureTool"],"disallowedTools":["Bash"],"skills":["synthetic"],
        "injectAgentsMd":false,"permissionMode":"plan","maxTurns":0,"background":false,
        "mcpServers":[{"name":"fixture","nested":{"supported":true}}],"readOnly":false,
        "projectPath":"fixture/project","pluginId":"fixture@store","pluginName":"fixture",
        "diagnostics":[{"code":"fixture-warning","message":"Synthetic","path":"fixture.md"}]
    }).as_object().unwrap().clone());
    let raw = json!({"agents":[agent.clone()],"userAgents":[agent],"pluginAgents":[],
        "capability":{"userScopeAvailable":false,"userScopeReason":"desktop_only"},
        "diagnostics":[{"code":"fixture","message":"Synthetic"}]});
    let parsed: AgentsListResult = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), raw);
}

#[test]
fn required_lists_and_essential_agent_fields_cannot_silently_default() {
    let raw = json!({"agents":[],"userAgents":[],"pluginAgents":[],
        "capability":{"userScopeAvailable":true}});
    for field in ["agents", "userAgents", "pluginAgents", "capability"] {
        let mut invalid = raw.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<AgentsListResult>(invalid).is_err(),
            "{field}"
        );
    }
    for field in [
        "id",
        "name",
        "description",
        "systemPrompt",
        "path",
        "scope",
        "source",
        "enabled",
    ] {
        let mut invalid = raw_agent("fixture", "user", "user");
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<AgentSummary>(invalid).is_err(),
            "{field}"
        );
    }
    assert!(serde_json::from_value::<AgentsCapability>(json!({})).is_err());
}

#[test]
fn discriminants_optional_types_and_structured_selection_are_strict() {
    for (field, value) in [
        ("scope", json!("project")),
        ("source", json!("custom")),
        ("color", json!("gray")),
        ("permissionMode", json!("ask")),
        ("readOnly", json!("true")),
        ("modelSelection", json!("fixture/model")),
        (
            "modelSelection",
            json!({"providerId":"p","modelId":"m","extra":true}),
        ),
        (
            "modelSelection",
            json!({"providerId":"p","modelId":"m","options":{"effort":"high"}}),
        ),
    ] {
        let mut invalid = raw_agent("fixture", "user", "user");
        invalid[field] = value;
        assert!(
            serde_json::from_value::<AgentSummary>(invalid).is_err(),
            "{field}"
        );
    }
    assert!(
        serde_json::from_value::<AgentsCapability>(json!({
        "userScopeAvailable":false,"userScopeReason":"unsupported"}))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ModelSelection>(json!({"providerId":"  ","modelId":"m"})).is_err()
    );
}

#[test]
fn settings_discards_runtime_aliases_and_deduplicates_by_full_identity() {
    let canonical = raw_agent("plugin:fixture@store:helper", "user", "plugin");
    let mut alias = canonical.clone();
    alias["name"] = json!("helper");
    let user = raw_agent("same", "user", "user");
    let mut first = raw_agent("same", "workspace", "user");
    first["projectPath"] = json!("fixture/first");
    let mut second = first.clone();
    second["projectPath"] = json!("fixture/second");
    let data = snapshot(
        vec![alias, user, first, second],
        vec![canonical.clone(), canonical],
    );
    let rows = data.settings_rows(&workspace("fixture/current", Some(" fixture-identity ")));
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .filter(|row| row.agent.source == AgentSource::Plugin)
            .count(),
        1
    );
    assert_ne!(rows[1].key, rows[2].key);
    assert_ne!(rows[2].key, rows[3].key);
    assert_eq!(workspace("a", Some(" key ")).identity_key(), "key");
    assert_eq!(workspace("a", Some(" ")).identity_key(), "a");
}

#[test]
fn filtering_matches_user_and_project_reference_without_scope_leaks() {
    let builtin = raw_agent("builtin", "built-in", "built-in");
    let user = raw_agent("user", "user", "user");
    let missing_project = raw_agent("legacy", "workspace", "user");
    let mut project = raw_agent("project", "workspace", "user");
    project["projectPath"] = json!("fixture/first");
    let mut other = project.clone();
    other["projectPath"] = json!("fixture/second");
    let mut plugin = raw_agent("plugin:first", "workspace", "plugin");
    plugin["projectPath"] = json!("fixture/first");
    let data = snapshot(
        vec![builtin, user, missing_project, project, other],
        vec![
            plugin,
            raw_agent("plugin:user", "user", "plugin"),
            raw_agent("plugin:missing", "workspace", "plugin"),
        ],
    );
    assert_eq!(data.visible_rows(&SettingsScope::User, "").len(), 3);
    let rows = data.visible_rows(
        &SettingsScope::Workspace(workspace("fixture/first", None)),
        "",
    );
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|row| row.agent.scope == AgentScope::Workspace)
    );
}

#[test]
fn color_labels_use_canonical_wire_values() {
    for color in [
        AgentColor::Red,
        AgentColor::Blue,
        AgentColor::Green,
        AgentColor::Yellow,
        AgentColor::Purple,
        AgentColor::Orange,
        AgentColor::Pink,
        AgentColor::Cyan,
    ] {
        assert_eq!(serde_json::to_value(color).unwrap(), json!(color.as_str()));
    }
}

#[test]
fn action_permissions_are_source_scope_and_readonly_aware() {
    for scope in ["user", "workspace", "built-in"] {
        for source in ["user", "plugin", "built-in"] {
            for readonly in [false, true] {
                let mut raw = raw_agent("fixture", scope, source);
                raw["readOnly"] = json!(readonly);
                let agent: AgentSummary = serde_json::from_value(raw).unwrap();
                let editable = source == "user" && scope != "built-in" && !readonly;
                assert_eq!(agent.can_edit(), editable);
                assert_eq!(agent.can_delete(), editable);
                assert_eq!(agent.can_toggle_enabled(), editable && scope == "user");
                assert_eq!(agent.override_target().is_some(), source == "plugin");
            }
        }
    }
    let mut raw = raw_agent("builtin", "built-in", "built-in");
    raw["name"] = json!("Explore");
    assert!(
        serde_json::from_value::<AgentSummary>(raw)
            .unwrap()
            .override_target()
            .is_some()
    );
}

#[test]
fn search_and_group_helpers_preserve_stable_rows_and_plugin_store_identity() {
    let mut user = raw_agent("user", "user", "user");
    user["tools"] = json!(["FixtureTool"]);
    user["skills"] = json!(["FixtureSkill"]);
    user["path"] = json!("fixture/user-only.md");
    user["modelSelection"] = json!({"providerId":"fixture-provider","modelId":"fixture-model"});
    let mut first = raw_agent("plugin:first", "user", "plugin");
    first["pluginId"] = json!("same@first");
    let mut second = first.clone();
    second["id"] = json!("plugin:second");
    second["pluginId"] = json!("same@second");
    let data = snapshot(vec![user], vec![first, second]);
    for query in [
        " fixturetool ",
        "FIXTURESKILL",
        "fixture-provider/fixture-model",
        "fixture/user-only.md",
    ] {
        assert_eq!(data.visible_rows(&SettingsScope::User, query).len(), 1);
    }
    let rows = data.visible_rows(&SettingsScope::User, "");
    let groups = group_rows(&rows);
    assert_eq!(groups.user.len(), 1);
    assert_eq!(groups.plugin.len(), 2);
    assert_eq!(group_plugin_rows(&groups.plugin).len(), 2);
}
