use super::profile_host_support::{ScratchHost, named};
use crate::app::subagent_profiles::{AgentScope, FormDraft, SubAgentConfig};
use serde_json::{Value, json};
use std::path::Path;

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_profiles_roundtrip_and_restart() {
    let mut host = ScratchHost::new();
    assert!(
        host.user_list()
            .agents
            .iter()
            .any(|agent| agent.config.name == "Explore")
    );
    let selection = host.selection();
    let config = json!({
        "name": "scratch-agent", "description": "Native scratch profile",
        "systemPrompt": "Disposable native test sentinel\nSecond prompt line.",
        "color": "purple", "modelSelection": selection,
        "tools": ["Read", "Bash(git status: native)", "FutureTool"],
        "disallowedTools": ["Write", "FutureDeniedTool"], "injectAgentsMd": false,
        "skills": ["unknown-test-skill"], "permissionMode": "plan", "maxTurns": 17,
        "background": true,
        "mcpServers": ["scratch-mcp", {"native-mcp": {
            "command": "disposable-command", "args": ["--fixture"],
            "env": {"NATIVE_SENTINEL": "synthetic"}, "enabled": false,
        }}],
    });
    let expected: SubAgentConfig = serde_json::from_value(config.clone()).unwrap();
    let created = host.create("user", &host.isolated.workspace(), config);
    assert_eq!(created.config, expected);
    assert_eq!(created.scope, AgentScope::User);
    assert!(created.can_toggle_enabled());
    host.undefined(
        "setEnabled",
        json!({"agentId": created.id, "enabled": false}),
    );
    host.stop();

    // 当前服务只投影已支持字段，重建 YAML 会丢弃未知字段；测试刻画限制，不伪称保真保存。
    let path = Path::new(&created.path);
    let original = std::fs::read_to_string(path).unwrap();
    let fixture = original.replacen(
        "---\n",
        "---\n# native-unknown-comment\nnativeUnknown:\n  sentinel: disposable-yaml\n",
        1,
    );
    host.write_fixture(path, fixture.as_bytes());
    host.start();
    let snapshot = host.user_list();
    let loaded = named(&snapshot.agents, "scratch-agent");
    assert_eq!(loaded.config, expected);
    assert!(!loaded.enabled);
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        fixture,
        "hydration must retain unknown YAML bytes"
    );
    let raw = host.json("list", json!({
        "workspacePath": host.isolated.workspace().to_string_lossy(), "mode": "settingsUserOnly",
    }));
    let raw_agent = raw["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["name"] == "scratch-agent")
        .unwrap();
    assert!(
        raw_agent.get("nativeUnknown").is_none(),
        "unknown YAML is not in the current public summary"
    );

    let mut draft = FormDraft::edit(loaded).unwrap();
    draft.fields.description = "Native edited description".into();
    let edited = draft.config().unwrap();
    let updated = host.update(loaded, serde_json::to_value(&edited).unwrap());
    assert_eq!(
        updated.config, edited,
        "all untouched supported metadata must survive native serialization"
    );
    let saved = std::fs::read_to_string(path).unwrap();
    assert!(!saved.contains("nativeUnknown"));
    assert!(!saved.contains("disposable-yaml"));
    assert!(!saved.contains("native-unknown-comment"));
    host.restart();
    let snapshot = host.user_list();
    let loaded = named(&snapshot.agents, "scratch-agent");
    assert_eq!(loaded.config, edited);
    assert!(!loaded.enabled);

    let mut renamed = serde_json::to_value(&loaded.config).unwrap();
    renamed["name"] = json!("renamed-agent");
    let renamed = host.update(loaded, renamed);
    assert_ne!(renamed.id, created.id);
    assert!(!path.exists(), "rename must remove the old canonical file");
    host.restart();
    let snapshot = host.user_list();
    assert!(
        !snapshot
            .agents
            .iter()
            .any(|agent| agent.config.name == "scratch-agent")
    );
    let loaded = named(&snapshot.agents, "renamed-agent");
    assert!(
        !loaded.enabled,
        "disabled identity must migrate through rename and restart"
    );
    let mut expected_renamed = edited;
    expected_renamed.name = "renamed-agent".into();
    assert_eq!(loaded.config, expected_renamed);
    host.undefined(
        "deleteAgent",
        json!({"agentId": loaded.id, "filePath": loaded.path}),
    );
    assert!(!Path::new(&loaded.path).exists());
    host.restart();
    assert!(
        !host
            .user_list()
            .agents
            .iter()
            .any(|agent| agent.config.name == "renamed-agent")
    );
    assert!(
        host.state()["disabledAgentIds"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_lost_reply_is_reviewed_without_replay() {
    let mut host = ScratchHost::new();
    let config = json!({"name":"lost-reply-agent","description":"Synthetic outcome","systemPrompt":"Synthetic prompt"});
    host.discard_reply("createAgent", json!({"provider":"glm","scope":"user","workspacePath":host.isolated.workspace(),"config":config}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if host
            .user_list()
            .agents
            .iter()
            .any(|a| a.config.name == "lost-reply-agent")
        {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    host.restart();
    let snapshot = host.user_list();
    let mutation = super::profile_state::ProfileMutation {
        origin: Default::default(),
        receipt: "unacknowledged".into(),
        scope: "user".into(),
        method: "createAgent".into(),
        params: json!({"config":config}),
        baseline: None,
    };
    assert!(super::profile_recovery::observed(&mutation, &snapshot));
    assert_eq!(
        snapshot
            .agents
            .iter()
            .filter(|a| a.config.name == "lost-reply-agent")
            .count(),
        1
    );
}

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_builtin_full_override_set_clear_and_restart() {
    let mut host = ScratchHost::new();
    let selection = host.selection();
    assert!(selection.reasoning_level().is_some());
    for name in ["Explore", "general-purpose"] {
        let snapshot = host.user_list();
        let builtin = named(&snapshot.agents, name);
        assert!(builtin.is_builtin());
        assert!(!builtin.can_edit());
        assert!(!builtin.can_toggle_enabled());
        assert!(builtin.config.model_selection.is_none());
        host.undefined(
            "setBuiltInModelOverride",
            json!({
                "agentName": name, "modelSelection": selection,
            }),
        );
    }
    assert_builtin_selections(&host, Some(&selection));
    host.restart();
    assert_builtin_selections(&host, Some(&selection));
    for name in ["Explore", "general-purpose"] {
        assert_eq!(
            host.state()["builtInModelSelectionOverrides"][name],
            json!(selection)
        );
        // 清除省略整份选择，不能发送 null 或留下模型/旧档位。
        host.undefined("setBuiltInModelOverride", json!({"agentName": name}));
    }
    assert_builtin_selections(&host, None);
    assert!(
        host.state()["builtInModelSelectionOverrides"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    host.restart();
    assert_builtin_selections(&host, None);
    assert!(
        host.state()["builtInModelSelectionOverrides"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

fn assert_builtin_selections(
    host: &ScratchHost,
    expected: Option<&crate::app::subagent_profiles::ModelSelection>,
) {
    let snapshot = host.user_list();
    for name in ["Explore", "general-purpose"] {
        let builtin = named(&snapshot.agents, name);
        assert_eq!(builtin.config.model_selection.as_ref(), expected);
        assert_eq!(builtin.model_selection_override.as_ref(), expected);
        assert!(builtin.default_model_selection.is_none());
    }
}

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_same_name_user_and_two_workspace_inventories() {
    let mut host = ScratchHost::new();
    let project_a = host.isolated.workspace().join("project-a");
    let project_b = host.isolated.workspace().join("project-b");
    for project in [&project_a, &project_b] {
        std::fs::create_dir(project).unwrap();
        host.owned_path(project);
    }
    let config = |description: &str| -> Value {
        json!({
            "name": "same-name", "description": description, "systemPrompt": "Native scope sentinel",
            "tools": ["Read"],
        })
    };
    let user = host.create("user", &project_a, config("user-only"));
    let workspace_a = host.create("workspace", &project_a, config("project-a-only"));
    let workspace_b = host.create("workspace", &project_b, config("project-b-only"));
    assert_ne!(user.id, workspace_a.id);
    // 当前工作区 ID 不包含路径；验证 inventory 按 cwd 隔离，不能拿此 ID 当跨工作区唯一键。
    assert_eq!(workspace_a.id, workspace_b.id);
    assert_ne!(workspace_a.path, workspace_b.path);
    assert!(user.can_toggle_enabled());
    assert!(workspace_a.can_edit() && workspace_a.can_delete());
    assert!(!workspace_a.can_toggle_enabled());
    assert!(!workspace_b.can_toggle_enabled());
    host.undefined("setEnabled", json!({"agentId": user.id, "enabled": false}));
    for restarted in [false, true] {
        if restarted {
            host.restart();
        }
        for (project, workspace, description) in [
            (&project_a, &workspace_a, "project-a-only"),
            (&project_b, &workspace_b, "project-b-only"),
        ] {
            let settings = host.list(project, "settingsUserOnly");
            assert_eq!(settings.user_agents.len(), 1);
            let listed = named(&settings.user_agents, "same-name");
            assert_eq!(listed.scope, AgentScope::User);
            assert_eq!(listed.path, user.path);
            assert_eq!(listed.config.description, "user-only");
            assert!(!listed.enabled);
            assert!(
                settings
                    .agents
                    .iter()
                    .all(|agent| agent.scope != AgentScope::Workspace)
            );
            let runtime = host.list(project, "allRuntimeScopes");
            assert_eq!(runtime.user_agents.len(), 1);
            let listed = named(&runtime.agents, "same-name");
            assert_eq!(listed.scope, AgentScope::Workspace);
            assert_eq!(listed.path, workspace.path);
            assert_eq!(listed.project_path.as_deref(), project.to_str());
            assert_eq!(listed.config.description, description);
            assert!(
                listed.enabled,
                "disabled user must not disable the workspace winner"
            );
        }
    }
}
