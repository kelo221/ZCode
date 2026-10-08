//! Test-only scratch fixtures; management always goes through the unchanged Host.
use crate::app::subagent_profiles::{
    AgentSummary, AgentsListResult, ModelSelection, ModelSelectionView,
};
use crate::backend::services_rpc::{ServiceClient, ServiceValue};
use crate::shared::isolation::IsolatedSettings;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub(super) struct ScratchHost {
    pub(super) isolated: Arc<IsolatedSettings>,
    client: Option<ServiceClient>,
}

impl ScratchHost {
    pub(super) fn new() -> Self {
        let mut host = Self {
            isolated: IsolatedSettings::create().unwrap(),
            client: None,
        };
        // 全新账户目录只提供不可执行的账户模型；先写 canonical 合成直连配置再启动，
        // 否则覆盖测试在 getView 的真实可用模型筛选处失败，不能放宽筛选伪造选择。
        host.install_personal_provider_fixture();
        host.start();
        host
    }

    fn install_personal_provider_fixture(&self) {
        // Services 按 ZCODE_DATA_BASE_DIR 读取 v2/provider_config.json；不继承真实配置/凭据。
        // 完整 API schema 要求非空 key，此值仅是合成占位；测试只读元数据，不调用连通性/API。
        let fixture = json!({
            "schemaVersion": 1,
            "config": {
                "providerConfigRules": {"providerRules": [{
                    "providerId": "native-acceptance", "providerName": "Native metadata fixture",
                    "enabled": true,
                    "config": {
                        "group": "standard-personal",
                        "access": {"type": "api-key", "apiKey": "native-test-placeholder-not-a-credential"},
                        "api": {"type": "openai-chat-completions", "baseUrl": "http://127.0.0.1:1/v1"},
                        "personalModelIds": ["native-metadata"],
                    },
                }]},
                "modelConfigRules": {
                    "providerModelRules": [{
                        "providerId": "native-acceptance", "modelId": "native-metadata",
                        "config": {
                            "enabled": true,
                            "properties": {
                                "requiresMfjsToolSchema": false, "contextWindow": 8192,
                                "inputFormat": {
                                    "supportsText": true, "supportsImage": true, "supportsVideo": false,
                                    "supportsAudio": false, "supportsPdf": false,
                                },
                                "outputFormat": {"supportsText": true}, "supportsToolCall": true,
                                "supportsJsonSchemaOutput": false, "supportsNativeWebSearch": false,
                                "supportsMidConversationSystem": true,
                            },
                            "optionSpecs": {
                                "reasoningLevel": {"values": ["low", "high"], "map": "{\"reasoning_effort\": reasoningLevel}"},
                                "maxOutputTokens": {"max": 2048, "map": "{\"max_tokens\": maxOutputTokens}"},
                            },
                        },
                    }],
                    "manualProviderModelRules": [],
                },
            },
        });
        self.write_fixture(
            &self
                .isolated
                .data_base()
                .join(".zcode/v2/provider_config.json"),
            &serde_json::to_vec(&fixture).unwrap(),
        );
    }

    pub(super) fn start(&mut self) {
        assert!(self.client.is_none(), "previous owned Host must stop first");
        let launch = crate::backend::services_launch::resolve(&self.isolated).unwrap();
        self.client = Some(
            ServiceClient::spawn(
                Path::new(&launch.program),
                &launch.args,
                &launch.envs,
                &self.isolated.workspace(),
            )
            .unwrap(),
        );
    }

    pub(super) fn stop(&mut self) {
        if let Some(mut client) = self.client.take() {
            client.shutdown();
            // 重启/改 fixture 必须核验整个 owned tree；仅根 PID 消失不能证明配置写入已停止。
            client.wait_for_exit(Duration::from_secs(10)).unwrap();
        }
    }

    pub(super) fn restart(&mut self) {
        self.stop();
        self.start();
    }

    pub(super) fn discard_reply(&self, method: &str, params: Value) {
        let call = self
            .client
            .as_ref()
            .unwrap()
            .call("subagents", method, vec![params])
            .unwrap();
        drop(call);
    }

    pub(super) fn try_call(
        &self,
        channel: &str,
        method: &str,
        args: Vec<Value>,
    ) -> Result<ServiceValue, String> {
        let client = self.client.as_ref().expect("owned Host is stopped");
        let request = client.call(channel, method, args)?;
        let result = futures::executor::block_on(request.receiver)
            .map_err(|_| "Services reply was dropped".to_owned())?;
        assert_eq!(client.pending_count(), 0);
        result
    }

    pub(super) fn call(&self, channel: &str, method: &str, args: Vec<Value>) -> ServiceValue {
        self.try_call(channel, method, args).unwrap()
    }

    pub(super) fn json(&self, method: &str, params: Value) -> Value {
        self.call("subagents", method, vec![params])
            .into_json()
            .unwrap()
    }

    pub(super) fn undefined(&self, method: &str, params: Value) {
        assert_eq!(
            self.call("subagents", method, vec![params]),
            ServiceValue::Undefined
        );
    }

    pub(super) fn list(&self, workspace: &Path, mode: &str) -> AgentsListResult {
        assert!(self.isolated.allows_workspace(workspace));
        let result: AgentsListResult = serde_json::from_value(self.json(
            "list",
            json!({
                "workspacePath": workspace.to_string_lossy(), "mode": mode,
            }),
        ))
        .unwrap();
        assert!(result.capability.user_scope_available);
        assert!(result.diagnostics.as_ref().is_none_or(Vec::is_empty));
        result
    }

    pub(super) fn user_list(&self) -> AgentsListResult {
        self.list(&self.isolated.workspace(), "settingsUserOnly")
    }

    pub(super) fn selection(&self) -> ModelSelection {
        let view: ModelSelectionView = serde_json::from_value(
            self.call("model-selection", "getView", vec![])
                .into_json()
                .unwrap(),
        )
        .unwrap();
        let selection = view
            .providers
            .iter()
            .find_map(|provider| {
                provider.models.iter().find_map(|model| {
                    view.complete_selection(&provider.provider_id, &model.model_id)
                        .ok()
                })
            })
            .expect("scratch Host must publish a usable model and reasoning level");
        view.validate_selection(&selection).unwrap();
        selection
    }

    pub(super) fn create(&self, scope: &str, workspace: &Path, config: Value) -> AgentSummary {
        assert!(self.isolated.allows_workspace(workspace));
        let result = self.json(
            "createAgent",
            json!({
                "provider": "glm", "scope": scope,
                "workspacePath": workspace.to_string_lossy(), "config": config,
            }),
        );
        let agent: AgentSummary = serde_json::from_value(result["agent"].clone()).unwrap();
        self.owned_path(Path::new(&agent.path));
        agent
    }

    pub(super) fn update(&self, agent: &AgentSummary, config: Value) -> AgentSummary {
        self.owned_path(Path::new(&agent.path));
        let workspace = agent
            .project_path
            .as_deref()
            .map(Path::new)
            .unwrap_or_else(|| self.isolated.root());
        assert!(self.isolated.allows_workspace(workspace));
        let result = self.json(
            "updateAgent",
            json!({
                "provider": "glm", "scope": agent.scope.as_str(), "agentId": agent.id,
                "oldFilePath": agent.path, "workspacePath": workspace.to_string_lossy(),
                "config": config,
            }),
        );
        let updated: AgentSummary = serde_json::from_value(result["agent"].clone()).unwrap();
        self.owned_path(Path::new(&updated.path));
        updated
    }

    pub(super) fn owned_path(&self, path: &Path) {
        assert!(path.is_absolute());
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(self.isolated.root())
        );
    }

    pub(super) fn write_fixture(&self, path: &Path, bytes: &[u8]) {
        assert!(
            self.client.is_none(),
            "fixture writes require a stopped owned Host"
        );
        assert!(path.starts_with(self.isolated.root()));
        assert!(
            !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        self.owned_path(path.parent().unwrap());
        if path.exists() {
            self.owned_path(path);
        }
        std::fs::write(path, bytes).unwrap();
    }

    pub(super) fn state_file(&self) -> PathBuf {
        self.isolated.home().join(".zcode/v2/agents-state.json")
    }

    pub(super) fn state(&self) -> Value {
        let path = self.state_file();
        self.owned_path(&path);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }
}

impl Drop for ScratchHost {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) fn named<'a>(agents: &'a [AgentSummary], name: &str) -> &'a AgentSummary {
    let matching: Vec<_> = agents
        .iter()
        .filter(|agent| agent.config.name == name)
        .collect();
    assert_eq!(matching.len(), 1, "expected one profile named {name}");
    matching[0]
}
