//! Model / thinking-level / collaboration-mode switching (CAS acks are handled
//! in backend/session_cmds.rs) and the effective config projection used by the
//! selectors.

use crate::app::store::AppState;
use crate::backend::workspace::CommandCtx;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    /// Switch model (and optionally thinking level) on the active session.
    /// `thought` empty string means "no explicit thought".
    pub fn switch_model(
        &mut self,
        provider: &str,
        model: &str,
        thought: &str,
        cx: &mut Context<Self>,
    ) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let payload = json!({ "provider": provider, "model": model, "thought": thought });
        self.send_session_command(
            &ws_key,
            CommandCtx::new(&sid, "switchModelConfig", payload).cas(),
        );
        cx.notify();
    }

    /// Thinking-level-only switch: same provider/model, different `thought`.
    pub fn switch_thought(&mut self, thought: &str, cx: &mut Context<Self>) {
        let Some(c) = self.active_conversation() else {
            return;
        };
        let (provider, model) = (c.config.provider.clone(), c.config.model.clone());
        self.switch_model(&provider, &model, thought, cx);
    }

    /// Collaboration mode: build | edit | plan | yolo ("Full access").
    pub fn switch_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let payload = json!({ "mode": mode });
        self.send_session_command(
            &ws_key,
            CommandCtx::new(&sid, "switchCollaborationMode", payload).cas(),
        );
        cx.notify();
    }

    pub(crate) fn find_model_option(
        &self,
        value: &str,
    ) -> Option<crate::composer::catalog::ModelOption> {
        let key = self.active_ws_key()?;
        self.workspace_configs
            .get(&key)?
            .models
            .iter()
            .find(|m| m.value == value)
            .cloned()
    }

    /// createSession.config override assembled from draft UI selections.
    pub(crate) fn draft_config(&self) -> Value {
        let mut config = json!({});
        if let Some(value) = &self.ui_model_value
            && let Some(opt) = self.find_model_option(value)
        {
            let thought = if opt.thought_levels.is_empty() {
                String::new()
            } else {
                opt.default_thought.clone()
            };
            config["provider"] = json!(opt.provider);
            config["model"] = json!(opt.model);
            if !thought.is_empty() {
                config["thought"] = json!(thought);
            }
        }
        if let Some(mode) = &self.ui_mode {
            if mode == "plan" {
                // Desktop parity: plan is submitted as build + planEnabled
                // (packages/ui/src/v4/composer/composerSubmissionConfig.ts).
                config["mode"] = json!("build");
                config["planEnabled"] = json!(true);
            } else {
                config["mode"] = json!(mode);
            }
        }
        config
    }

    /// Effective session config for rendering: live session config, else the
    /// draft selections, else workspace defaults from the config topic.
    pub fn effective_config(&self) -> EffectiveConfig {
        if let Some(c) = self.active_conversation() {
            return EffectiveConfig {
                provider: c.config.provider.clone(),
                model: c.config.model.clone(),
                model_label: String::new(),
                thought: c.config.thought.clone(),
                thought_levels: c.config.thought_levels.clone(),
                mode: c.config.mode.clone(),
                has_session: true,
            };
        }
        let ws_key = self.active_ws_key();
        let cfg = ws_key
            .as_deref()
            .and_then(|k| self.workspace_configs.get(k));
        // Draft model: UI choice wins, else the workspace default entry.
        let (model_label, provider, model, thought_levels) =
            if let Some(value) = &self.ui_model_value {
                match self.find_model_option(value) {
                    Some(opt) => (
                        opt.name.clone(),
                        opt.provider.clone(),
                        opt.model.clone(),
                        opt.thought_levels.clone(),
                    ),
                    None => (value.clone(), String::new(), String::new(), vec![]),
                }
            } else if let Some(ws_cfg) = cfg {
                let default_value = ws_cfg
                    .raw_options
                    .iter()
                    .find(|o| o.get("id").and_then(Value::as_str) == Some("model"))
                    .and_then(|o| o.get("currentValue"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match ws_cfg.models.iter().find(|m| m.value == default_value) {
                    Some(opt) => (
                        opt.name.clone(),
                        opt.provider.clone(),
                        opt.model.clone(),
                        opt.thought_levels.clone(),
                    ),
                    None => (String::new(), String::new(), String::new(), vec![]),
                }
            } else {
                (String::new(), String::new(), String::new(), vec![])
            };
        let mode = self
            .ui_mode
            .clone()
            .or_else(|| cfg.map(|c| c.default_mode.clone()))
            .unwrap_or_else(|| "build".into());
        EffectiveConfig {
            provider,
            model,
            model_label,
            thought: String::new(),
            thought_levels,
            mode,
            has_session: false,
        }
    }
}

#[derive(Clone, Default)]
pub struct EffectiveConfig {
    pub provider: String,
    pub model: String,
    pub model_label: String,
    pub thought: String,
    pub thought_levels: Vec<String>,
    pub mode: String,
    pub has_session: bool,
}

impl EffectiveConfig {
    pub fn model_display(&self) -> String {
        if !self.model_label.is_empty() {
            self.model_label.clone()
        } else if !self.model.is_empty() {
            self.model.clone()
        } else {
            "model…".into()
        }
    }

    /// Trigger label with the catalog's display name (desktop parity:
    /// `resolveV4ModelTriggerDisplay` — builtin/family providers show the bare
    /// model name; custom providers get a `Provider/Model` prefix, only meaningful
    /// when the catalog actually has more than one provider). Falls back to
    /// `model_display` when the current model is not in the catalog
    /// (`<synthetic>`, retired ids, …).
    pub fn trigger_display(&self, models: &[crate::composer::catalog::ModelOption]) -> String {
        let matched = models
            .iter()
            .find(|o| o.provider == self.provider && o.model == self.model);
        let Some(opt) = matched else {
            return self.model_display();
        };
        let multi_provider = models.iter().any(|o| o.provider != self.provider);
        if multi_provider && !opt.provider_name.is_empty() {
            format!("{}/{}", opt.provider_name, opt.name)
        } else {
            opt.name.clone()
        }
    }

    pub fn thought_display(&self) -> String {
        let v = self.thought.trim().to_lowercase();
        match v.as_str() {
            "disabled" | "false" | "no" | "none" | "off" | "nothink" | "no-think" | "no_think" => {
                "Off".into()
            }
            "enable" | "enabled" | "on" | "true" => "On".into(),
            "low" => "Low".into(),
            "minimal" => "Minimal".into(),
            "medium" => "Medium".into(),
            "high" => "High".into(),
            "max" => "Max".into(),
            "ultra" => "Ultra".into(),
            "extra-high" | "extra_high" | "xhigh" => "X-High".into(),
            "" => "Off".into(),
            _ => title_case(&self.thought),
        }
    }
}

/// "medium" -> "Medium" (thinking-level labels, as in the desktop picker).
pub fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thought_display_mapping() {
        let display = |t: &str| {
            EffectiveConfig {
                thought: t.into(),
                ..Default::default()
            }
            .thought_display()
        };

        assert_eq!(display("enabled"), "On");
        assert_eq!(display("on"), "On");
        assert_eq!(display("disabled"), "Off");
        assert_eq!(display("off"), "Off");
        assert_eq!(display(""), "Off");
        assert_eq!(display("high"), "High");
        assert_eq!(display("xhigh"), "X-High");
    }
}
