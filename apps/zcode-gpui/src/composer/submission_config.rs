use crate::app::store::AppState;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn composer_draft_key(&self) -> String {
        self.active
            .clone()
            .unwrap_or_else(|| format!("draft:{}", self.active_workspace.as_deref().unwrap_or("")))
    }
    pub(crate) fn submission_override(&self) -> Value {
        self.draft_submission_overrides
            .get(&self.composer_draft_key())
            .cloned()
            .unwrap_or(Value::Null)
    }
    pub(crate) fn with_submission_override(&self, mut payload: Value) -> Value {
        if let (Some(target), Value::Object(config)) =
            (payload.as_object_mut(), self.submission_override())
        {
            target.extend(config);
        }
        payload
    }
    pub(crate) fn set_restored_model(
        &mut self,
        provider: &str,
        model: &str,
        thought: &str,
    ) -> bool {
        let key = self.composer_draft_key();
        let Some(config) = self.draft_submission_overrides.get_mut(&key) else {
            return false;
        };
        let same_model = config
            .pointer("/modelSelection/providerId")
            .and_then(Value::as_str)
            == Some(provider)
            && config
                .pointer("/modelSelection/modelId")
                .and_then(Value::as_str)
                == Some(model);
        let mut options = if same_model {
            config
                .pointer("/modelSelection/options")
                .cloned()
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };
        if thought.is_empty() {
            if let Some(options) = options.as_object_mut() {
                options.remove("reasoningLevel");
            }
        } else {
            options["reasoningLevel"] = json!(thought);
        }
        config["modelSelection"] = json!({"providerId":provider,"modelId":model,"options":options});
        true
    }
    pub(crate) fn set_restored_mode(&mut self, mode: &str) -> bool {
        let key = self.composer_draft_key();
        let Some(config) = self.draft_submission_overrides.get_mut(&key) else {
            return false;
        };
        // plan 是叠加层：选 plan 只开启 planEnabled，选权限模式不再顺带关闭计划模式。
        if mode == "plan" {
            if config.get("mode").is_none() {
                config["mode"] = json!("build");
            }
            config["planEnabled"] = json!(true);
        } else {
            config["mode"] = json!(mode);
        }
        true
    }
}
