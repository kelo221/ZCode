use super::profile_host_support::{ScratchHost, named};
use crate::app::subagent_profiles::{AgentSource, ModelSelection};
use serde_json::json;
use std::path::{Path, PathBuf};

const PLUGIN_ID: &str = "profile-fixture@native-tests";
const AGENT_ID: &str = "plugin:profile-fixture@native-tests:review";
const PROFILE_NAME: &str = "profile-fixture:Review";

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_plugin_override_stable_version_identity_and_clear() {
    let mut host = ScratchHost::new();
    let selection = host.selection();
    host.stop();
    let v1 = install_fixture_version(&host, "1.0.0", "declared-v1", "low");
    let original_v1 = std::fs::read(&v1).unwrap();
    host.start();
    assert_plugin_selection(&host, "declared-v1", "low", None);
    let snapshot = host.user_list();
    assert_eq!(
        snapshot.plugin_agents.len(),
        1,
        "aliases must not duplicate settings inventory"
    );
    let profile = named(&snapshot.plugin_agents, PROFILE_NAME);
    assert_eq!(profile.id, AGENT_ID);
    assert_eq!(profile.source, AgentSource::Plugin);
    assert_eq!(profile.plugin_id.as_deref(), Some(PLUGIN_ID));
    assert_eq!(profile.read_only, Some(true));
    assert!(!profile.can_edit() && !profile.can_delete() && !profile.can_toggle_enabled());
    assert!(
        !snapshot
            .agents
            .iter()
            .any(|agent| agent.source == AgentSource::Plugin)
    );
    host.undefined(
        "setPluginAgentModelOverride",
        json!({
            "agentId": profile.id, "modelSelection": selection,
        }),
    );
    assert_plugin_selection(&host, "declared-v1", "low", Some(&selection));
    assert_eq!(
        std::fs::read(&v1).unwrap(),
        original_v1,
        "override cannot modify plugin source"
    );
    host.restart();
    assert_plugin_selection(&host, "declared-v1", "low", Some(&selection));
    assert_eq!(
        host.state()["pluginAgentModelSelectionOverrides"][AGENT_ID],
        json!(selection)
    );
    let runtime = host.list(&host.isolated.workspace(), "allRuntimeScopes");
    assert_eq!(runtime.plugin_agents.len(), 1);
    let canonical = named(&runtime.agents, PROFILE_NAME);
    let alias = named(&runtime.agents, "Review");
    assert_ne!(canonical.id, alias.id);
    assert_eq!(canonical.path, alias.path);
    assert_eq!(alias.config.model_selection.as_ref(), Some(&selection));

    // 版本只改变安装路径/声明默认值，状态仍以不含版本的公开 plugin 身份关联。
    host.stop();
    let v2 = install_fixture_version(&host, "2.0.0", "declared-v2", "high");
    let original_v2 = std::fs::read(&v2).unwrap();
    host.start();
    let snapshot = host.user_list();
    assert_eq!(snapshot.plugin_agents.len(), 1);
    let profile = named(&snapshot.plugin_agents, PROFILE_NAME);
    assert_eq!(profile.id, AGENT_ID);
    assert_eq!(Path::new(&profile.path), v2);
    assert_ne!(Path::new(&profile.path), v1);
    assert_plugin_selection(&host, "declared-v2", "high", Some(&selection));
    assert_eq!(std::fs::read(&v1).unwrap(), original_v1);
    assert_eq!(std::fs::read(&v2).unwrap(), original_v2);
    host.undefined(
        "setPluginAgentModelOverride",
        json!({"agentId": profile.id}),
    );
    assert_plugin_selection(&host, "declared-v2", "high", None);
    assert!(
        host.state()["pluginAgentModelSelectionOverrides"]
            .get(AGENT_ID)
            .is_none()
    );
    host.restart();
    assert_plugin_selection(&host, "declared-v2", "high", None);
    assert!(
        host.state()["pluginAgentModelSelectionOverrides"]
            .get(AGENT_ID)
            .is_none()
    );
    assert_eq!(std::fs::read(&v1).unwrap(), original_v1);
    assert_eq!(std::fs::read(&v2).unwrap(), original_v2);
}

fn assert_plugin_selection(
    host: &ScratchHost,
    model: &str,
    reasoning: &str,
    selection: Option<&ModelSelection>,
) {
    let snapshot = host.user_list();
    let profile = named(&snapshot.plugin_agents, PROFILE_NAME);
    assert_eq!(profile.id, AGENT_ID);
    let declared: ModelSelection = serde_json::from_value(json!({
        "providerId": "native-fixture", "modelId": model, "options": {"reasoningLevel": reasoning},
    }))
    .unwrap();
    assert_eq!(profile.default_model_selection.as_ref(), Some(&declared));
    assert_eq!(profile.model_selection_override.as_ref(), selection);
    assert_eq!(
        profile.config.model_selection.as_ref(),
        selection.or(Some(&declared))
    );
}

fn install_fixture_version(
    host: &ScratchHost,
    version: &str,
    model: &str,
    reasoning: &str,
) -> PathBuf {
    // 仅在已停止的 scratch Host 下写现有安装记录/manifest 格式，不增加生产文件写入路径。
    let cli = host.isolated.home().join(".zcode/cli");
    let plugin = cli
        .join("plugins/cache/native-tests/profile-fixture")
        .join(version);
    host.write_fixture(
        &plugin.join(".zcode-plugin/plugin.json"),
        &serde_json::to_vec(&json!({
            "name": "profile-fixture", "version": version, "agents": "./profiles",
        }))
        .unwrap(),
    );
    let profile = plugin.join("profiles/review.md");
    host.write_fixture(&profile, format!(
        "---\nname: Review\ndescription: Synthetic plugin profile\nmodel: native-fixture/{model}\nthoughtLevel: {reasoning}\ntools: [Read]\n---\n\nDisposable plugin sentinel.\n",
    ).as_bytes());
    host.write_fixture(
        &cli.join("plugins/installed_plugins.json"),
        &serde_json::to_vec(&json!({
            "version": 1,
            "plugins": [{"id": PLUGIN_ID, "name": "profile-fixture", "marketplace": "native-tests",
                "version": version, "scope": "user", "installPath": plugin.to_string_lossy()}],
        }))
        .unwrap(),
    );
    host.write_fixture(
        &cli.join("config.json"),
        &serde_json::to_vec(&json!({
            "plugins": {"enabledPlugins": {PLUGIN_ID: true}},
        }))
        .unwrap(),
    );
    profile
}
