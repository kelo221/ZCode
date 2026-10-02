//! Config catalog types: session config projection, model options, and the
//! workspace config state (models + default collaboration mode).
//!
//! Sources: packages/shared/src/zcode-protocol-v4/session-config.ts,
//! workspace-config.ts, packages/shared/src/model-selection.ts and the legacy
//! zcodeSessionSettingsStateSchema (packages/shared/src/zcode-protocol).

use serde_json::Value;

/// Session-level effective config projection (sessionConfigStateSchema):
/// `provider`/`model`/`thought` are the UI-facing values, `thoughtLevels`
/// are what the CURRENT model supports, `mode` ∈ build|edit|plan|yolo.
#[derive(Clone, Debug, Default)]
pub struct SessionConfig {
    pub provider: String,
    pub model: String,
    pub thought: String,
    pub thought_levels: Vec<String>,
    pub mode: String,
}

/// One selectable model.
#[derive(Clone, Debug)]
pub struct ModelOption {
    /// Raw option value, format `provider/model$thought`.
    pub value: String,
    pub name: String,
    pub provider_name: String,
    pub thought_levels: Vec<String>,
    pub default_thought: String,
    /// Parsed from `value`: `provider/model`.
    pub provider: String,
    pub model: String,
}

impl ModelOption {
    /// From the workspace-config catalog value shape
    /// (workspaceConfigSelectValueSchema).
    pub fn from_value(v: &Value) -> Option<Self> {
        let value = v.get("value")?.as_str()?.to_string();
        let (provider_model, thought_suffix) = match value.rsplit_once('$') {
            Some((left, right)) => (left, Some(right.to_string())),
            None => (value.as_str(), None),
        };
        let (provider, model) = provider_model.split_once('/')?;
        let str_or = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Some(ModelOption {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(model)
                .to_string(),
            provider_name: str_or("modelProviderName"),
            thought_levels: v
                .get("modelThoughtLevels")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            default_thought: thought_suffix.unwrap_or_else(|| str_or("modelDefaultThoughtLevel")),
            provider: provider.to_string(),
            model: model.to_string(),
            value,
        })
    }

    /// From the legacy session settings model option
    /// (zcodeModelOptionSchema: ref {providerId, modelId}, label,
    /// providerLabel, reasoning {levels, defaultLevel}).
    pub fn from_settings(v: &Value) -> Option<Self> {
        let provider = v
            .get("ref")
            .and_then(|r| r.get("providerId"))
            .and_then(Value::as_str)?
            .to_string();
        let model = v
            .get("ref")
            .and_then(|r| r.get("modelId"))
            .and_then(Value::as_str)?
            .to_string();
        let (thought_levels, default_thought) = match v.get("reasoning") {
            Some(r) => (
                r.get("levels")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|l| l.get("value").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                r.get("defaultLevel")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            None => (Vec::new(), String::new()),
        };
        Some(ModelOption {
            name: v
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(&model)
                .to_string(),
            provider_name: v
                .get("providerLabel")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            thought_levels,
            default_thought,
            provider: provider.clone(),
            model: model.clone(),
            value: format!("{provider}/{model}"),
        })
    }
}

/// workspace-config topic state (workspaceConfigStateSchema): whole-replacement.
#[derive(Clone, Debug, Default)]
pub struct WorkspaceConfig {
    pub models: Vec<ModelOption>,
    /// currentValue of the `mode` option (workspace default).
    pub default_mode: String,
    /// Raw option list for anything the minimal UI doesn't model yet.
    pub raw_options: Vec<Value>,
}

impl WorkspaceConfig {
    pub fn apply_state(&mut self, state: &Value) {
        let options = state
            .get("configOptions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut models = Vec::new();
        let mut default_mode = String::new();
        for opt in &options {
            let id = opt.get("id").and_then(Value::as_str).unwrap_or("");
            match id {
                "model" => {
                    if let Some(list) = opt.get("options").and_then(Value::as_array) {
                        models.extend(list.iter().filter_map(ModelOption::from_value));
                    }
                }
                "mode" => {
                    default_mode = opt
                        .get("currentValue")
                        .and_then(Value::as_str)
                        .unwrap_or("build")
                        .to_string();
                }
                _ => {}
            }
        }
        // The standalone CLI's workspace-config topic may carry an EMPTY model
        // list (the catalog is host-provided there); never clobber a catalog
        // harvested from the legacy session/create probe with empty data.
        if !models.is_empty() || self.models.is_empty() {
            self.models = models;
        }
        if !default_mode.is_empty() {
            self.default_mode = default_mode;
        }
        self.raw_options = options;
    }

    /// Merge the legacy session/create settings snapshot
    /// (zcodeSessionSettingsStateSchema) into the catalog.
    pub fn apply_settings(&mut self, settings: &Value) {
        if let Some(available) = settings
            .get("model")
            .and_then(|m| m.get("available"))
            .and_then(Value::as_array)
        {
            let models: Vec<ModelOption> = available
                .iter()
                .filter_map(ModelOption::from_settings)
                .collect();
            if !models.is_empty() {
                self.models = models;
            }
        }
        if let Some(mode) = settings.get("mode").and_then(|m| m.get("current"))
            && let Some(mode) = mode.as_str()
            && !mode.is_empty()
        {
            self.default_mode = mode.to_string();
        }
    }
}
