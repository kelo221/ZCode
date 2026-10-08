use serde::{Deserialize, Deserializer, Serialize, de};
use std::collections::HashSet;

fn trimmed_nonempty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    let value = super::form::backend_trim(&value);
    if value.is_empty() {
        return Err(de::Error::custom("model selection string is empty"));
    }
    Ok(value.into())
}

pub(super) fn optional_non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

fn optional_reasoning<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    trimmed_nonempty(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SelectionOptions {
    #[serde(
        default,
        deserialize_with = "optional_reasoning",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) reasoning_level: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelSelection {
    #[serde(deserialize_with = "trimmed_nonempty")]
    pub(crate) provider_id: String,
    #[serde(deserialize_with = "trimmed_nonempty")]
    pub(crate) model_id: String,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) options: Option<SelectionOptions>,
}

impl ModelSelection {
    pub(crate) fn reasoning_level(&self) -> Option<&str> {
        self.options.as_ref()?.reasoning_level.as_deref()
    }
    pub(crate) fn same_model(&self, provider: &str, model: &str) -> bool {
        self.provider_id == provider && self.model_id == model
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelSelectionView {
    pub(crate) revision: u64,
    pub(crate) providers: Vec<ModelProvider>,
    #[serde(
        default,
        deserialize_with = "optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) preferred_selection: Option<ModelSelection>,
}

// 只消费公开 facade 的必要事实；未知配置由 serde 跳过，不保留密钥、端点或 header。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", from = "ProviderWire")]
pub(crate) struct ModelProvider {
    pub(crate) provider_id: String,
    pub(crate) provider_name: Option<String>,
    pub(crate) template_id: Option<String>,
    pub(crate) models: Vec<ModelMetadata>,
    pub(crate) api_type: Option<ProviderApiType>,
    pub(crate) visibility: Option<ProviderVisibility>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderWire {
    provider_id: String,
    provider_name: Option<String>,
    template_id: Option<String>,
    models: Vec<ModelMetadata>,
    config: ProviderFacts,
}

#[derive(Deserialize)]
struct ProviderFacts {
    api: Option<ApiFacts>,
    visibility: Option<ProviderVisibility>,
}
#[derive(Deserialize)]
struct ApiFacts {
    #[serde(rename = "type")]
    kind: Option<ProviderApiType>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProviderApiType {
    AnthropicMessages,
    OpenaiChatCompletions,
    OpenaiResponses,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProviderVisibility {
    Visible,
    Hidden,
}

impl From<ProviderWire> for ModelProvider {
    fn from(wire: ProviderWire) -> Self {
        Self {
            provider_id: wire.provider_id,
            provider_name: wire.provider_name,
            template_id: wire.template_id,
            models: wire.models,
            api_type: wire.config.api.and_then(|api| api.kind),
            visibility: wire.config.visibility,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", from = "ModelWire")]
pub(crate) struct ModelMetadata {
    pub(crate) model_id: String,
    pub(crate) reasoning_levels: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelWire {
    model_id: String,
    config: ModelFacts,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelFacts {
    option_specs: OptionFacts,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionFacts {
    reasoning_level: ReasoningFacts,
}
#[derive(Deserialize)]
struct ReasoningFacts {
    #[serde(deserialize_with = "reasoning_values")]
    values: Vec<String>,
}

fn reasoning_values<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let values = Vec::<String>::deserialize(deserializer)?;
    let mut seen = HashSet::new();
    if values.is_empty()
        || values
            .iter()
            .any(|value| super::form::backend_trim(value).is_empty() || !seen.insert(value))
    {
        return Err(de::Error::custom("invalid published reasoning values"));
    }
    Ok(values)
}

impl From<ModelWire> for ModelMetadata {
    fn from(wire: ModelWire) -> Self {
        Self {
            model_id: wire.model_id,
            reasoning_levels: wire.config.option_specs.reasoning_level.values,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionError {
    ModelUnavailable,
    SelectionMissing,
    ReasoningMissing,
    ReasoningUnsupported,
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::ModelUnavailable => "Model is unavailable",
            Self::SelectionMissing => "Model selection is missing",
            Self::ReasoningMissing => "Reasoning level is required",
            Self::ReasoningUnsupported => "Reasoning level is unsupported",
        })
    }
}

impl ModelSelectionView {
    pub(crate) fn model(&self, provider: &str, model: &str) -> Option<&ModelMetadata> {
        self.providers
            .iter()
            .find(|item| {
                item.provider_id == provider
                    && item.api_type.is_some()
                    && item.visibility != Some(ProviderVisibility::Hidden)
            })?
            .models
            .iter()
            .find(|item| item.model_id == model)
    }
    pub(crate) fn complete_selection(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<ModelSelection, SelectionError> {
        let metadata = self
            .model(provider, model)
            .ok_or(SelectionError::ModelUnavailable)?;
        let reasoning = metadata
            .reasoning_levels
            .last()
            .ok_or(SelectionError::ReasoningMissing)?;
        Ok(ModelSelection {
            provider_id: provider.into(),
            model_id: model.into(),
            options: Some(SelectionOptions {
                reasoning_level: Some(reasoning.clone()),
            }),
        })
    }
    pub(crate) fn validate_selection(
        &self,
        selection: &ModelSelection,
    ) -> Result<(), SelectionError> {
        let metadata = self
            .model(&selection.provider_id, &selection.model_id)
            .ok_or(SelectionError::ModelUnavailable)?;
        let level = selection
            .reasoning_level()
            .ok_or(SelectionError::ReasoningMissing)?;
        if !metadata
            .reasoning_levels
            .iter()
            .any(|candidate| candidate == level)
        {
            return Err(SelectionError::ReasoningUnsupported);
        }
        Ok(())
    }
}

pub(super) fn select_model(
    selection: &mut Option<ModelSelection>,
    view: &ModelSelectionView,
    provider: &str,
    model: &str,
) -> Result<(), SelectionError> {
    if selection
        .as_ref()
        .is_some_and(|selected| selected.same_model(provider, model))
    {
        view.model(provider, model)
            .ok_or(SelectionError::ModelUnavailable)?;
        return Ok(());
    }
    // 旧档位属于旧模型；主动切换只按新模型的 canonical 顺序完成最高公开档。
    *selection = Some(view.complete_selection(provider, model)?);
    Ok(())
}

pub(super) fn set_reasoning(
    selection: &mut Option<ModelSelection>,
    view: &ModelSelectionView,
    reasoning: Option<&str>,
) -> Result<(), SelectionError> {
    let selection = selection.as_mut().ok_or(SelectionError::SelectionMissing)?;
    let metadata = view
        .model(&selection.provider_id, &selection.model_id)
        .ok_or(SelectionError::ModelUnavailable)?;
    let reasoning = reasoning.map(str::trim);
    if let Some(reasoning) = reasoning
        && !metadata
            .reasoning_levels
            .iter()
            .any(|value| value == reasoning)
    {
        return Err(SelectionError::ReasoningUnsupported);
    }
    selection.options = reasoning.map(|value| SelectionOptions {
        reasoning_level: Some(value.into()),
    });
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelChoice {
    pub(crate) selection: ModelSelection,
    pub(crate) reasoning_levels: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelGroup {
    pub(crate) provider_id: String,
    pub(crate) label: String,
    pub(crate) choices: Vec<ModelChoice>,
}

pub(crate) fn model_choices(view: &ModelSelectionView, query: &str) -> Vec<ModelGroup> {
    let query = query.trim().to_lowercase();
    view.providers
        .iter()
        .filter(|provider| {
            provider.api_type.is_some() && provider.visibility != Some(ProviderVisibility::Hidden)
        })
        .filter_map(|provider| {
            let label = provider
                .provider_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(&provider.provider_id)
                .to_owned();
            let choices: Vec<_> = provider
                .models
                .iter()
                .filter(|model| {
                    format!("{} {} {}", provider.provider_id, label, model.model_id)
                        .to_lowercase()
                        .contains(&query)
                })
                .filter_map(|model| {
                    view.complete_selection(&provider.provider_id, &model.model_id)
                        .ok()
                        .map(|selection| ModelChoice {
                            selection,
                            reasoning_levels: model.reasoning_levels.clone(),
                        })
                })
                .collect();
            (!choices.is_empty()).then(|| ModelGroup {
                provider_id: provider.provider_id.clone(),
                label,
                choices,
            })
        })
        .collect()
}
