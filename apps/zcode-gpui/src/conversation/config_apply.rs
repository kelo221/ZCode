//! Session `config` region mirror (sessionConfigStateSchema), split out of
//! model.rs for the 400-line cap.

use crate::conversation::model::ConversationState;
use serde_json::Value;

impl ConversationState {
    /// Whole-key replacement of the `config` region (sessionConfigStateSchema).
    pub fn apply_config(&mut self, config: &Value) {
        let str_field = |k: &str| {
            config
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        if config.get("provider").is_some() {
            self.config.provider = str_field("provider");
        }
        if config.get("model").is_some() {
            self.config.model = str_field("model");
        }
        if config.get("thought").is_some() {
            self.config.thought = str_field("thought");
        }
        if let Some(levels) = config.get("thoughtLevels").and_then(Value::as_array) {
            self.config.thought_levels = levels
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if config.get("mode").is_some() {
            let mode = str_field("mode");
            if mode == "plan" {
                // 旧格式把 plan 写进 mode；plan 是叠加层，权限模式保持不变（缺省 build）。
                self.config.plan_enabled = true;
                if self.config.mode.is_empty() {
                    self.config.mode = "build".into();
                }
            } else {
                self.config.mode = mode;
            }
        }
        // 此前只镜像 mode，丢弃了 planEnabled：Agent 进出计划模式或 /plan 发送后，
        // 模式按钮一直停在底层权限模式上，看起来“不更新”。
        if let Some(plan) = config.get("planEnabled").and_then(Value::as_bool) {
            self.config.plan_enabled = plan;
        }
        if let Some(id) = config
            .pointer("/planTransition/toolCallId")
            .and_then(Value::as_str)
        {
            self.config.plan_transition_id = Some(id.to_string());
        }
        if config.get("followupMode").is_some() {
            self.config.followup_mode = str_field("followupMode");
        }
    }
}
