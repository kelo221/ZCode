//! Draft → model config, ported from desktop
//! `resolveProviderModelDraftCommit` (`ProviderModelMetadata.ts`): smart mode
//! writes a sparse personal overlay, manual mode the materialized editable
//! leaves (desktop `extractManualModelConfig`).

use super::model_draft::{CAPABILITIES, CurrentModel, MODALITIES, ModelDraft, at};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// The field a draft fails on (desktop `ProviderModelDraftCommitResult.field`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DraftField {
    Id,
    ContextWindow,
    MaxOutputTokens,
    InputFormat,
    ReasoningLevelValues,
    ReasoningLevelMap,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DraftCommit {
    pub model_id: String,
    pub use_recommended: bool,
    pub config: Value,
}

type Object = Map<String, Value>;

fn object(value: Option<&Value>) -> Object {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn parse_positive(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok().filter(|n| *n > 0)
}

pub(crate) fn commit(
    draft: &ModelDraft,
    current: &CurrentModel,
) -> Result<DraftCommit, DraftField> {
    let model_id = draft.id.trim();
    if model_id.is_empty() {
        return Err(DraftField::Id);
    }
    let recommended = draft.recommended;
    let inherited = current.inherited.as_ref().filter(|_| recommended);
    let inherited_at = |path: &[&str]| inherited.and_then(|i| at(i, path));
    let config = &current.config;
    let personal = &current.personal;

    let context_window = if draft.context_window.trim().is_empty() {
        inherited_at(&["properties", "contextWindow"]).and_then(Value::as_u64)
    } else {
        parse_positive(&draft.context_window)
    }
    .ok_or(DraftField::ContextWindow)?;

    let max_output = if draft.max_output.trim().is_empty() {
        None
    } else {
        Some(parse_positive(&draft.max_output).ok_or(DraftField::MaxOutputTokens)?)
    };
    if !recommended && max_output.is_none() {
        return Err(DraftField::MaxOutputTokens);
    }
    if !draft.input_flag("supportsText") {
        return Err(DraftField::InputFormat);
    }

    let levels: Vec<String> = draft.levels.iter().map(|v| v.trim().to_owned()).collect();
    let unique: HashSet<&String> = levels.iter().collect();
    if levels.is_empty() || levels.iter().any(String::is_empty) || unique.len() != levels.len() {
        return Err(DraftField::ReasoningLevelValues);
    }
    let inherited_reasoning = inherited_at(&["optionSpecs", "reasoningLevel"]);
    let level_map = draft.level_map.trim();
    let inherited_map = inherited_reasoning
        .and_then(|r| at(r, &["map"]))
        .and_then(Value::as_str);
    // 映射表达式由 agent 端 compileModelOptionMap 编译校验；这里只保证存在可用映射。
    let effective_map = Some(level_map)
        .filter(|m| !m.is_empty())
        .or(inherited_map)
        .filter(|m| !m.is_empty())
        .ok_or(DraftField::ReasoningLevelMap)?
        .to_owned();

    let current_enabled = at(config, &["enabled"])
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let config_props = at(config, &["properties"]);
    let config_prop = |key: &str| config_props.and_then(|p| at(p, &[key])).cloned();
    let config_input = |key: &str| {
        config_props
            .and_then(|p| at(p, &["inputFormat", key]))
            .cloned()
    };

    // Effective properties (desktop `effectiveProperties`).
    let mut effective_props = Object::new();
    if let Some(v) = config_prop("requiresMfjsToolSchema") {
        effective_props.insert("requiresMfjsToolSchema".into(), v);
    }
    effective_props.insert("contextWindow".into(), json!(context_window));
    let mut effective_input = object(config_props.and_then(|p| at(p, &["inputFormat"])));
    for key in MODALITIES {
        effective_input.insert(key.into(), json!(draft.input_flag(key)));
    }
    effective_props.insert("inputFormat".into(), Value::Object(effective_input));
    for key in ["outputFormat", "supportsToolCall"] {
        if let Some(v) = config_prop(key) {
            effective_props.insert(key.into(), v);
        }
    }
    for (index, key) in CAPABILITIES.iter().enumerate() {
        effective_props.insert((*key).into(), json!(draft.capabilities[index]));
    }

    // Sparse personal properties (desktop `buildPersonalProperties`).
    let inherited_prop = |path: &[&str]| {
        let mut full = vec!["properties"];
        full.extend_from_slice(path);
        inherited_at(&full).cloned()
    };
    let mut personal_props = object(at(personal, &["properties"]));
    sparse_leaf(
        &mut personal_props,
        "contextWindow",
        Some(json!(context_window)),
        inherited_prop(&["contextWindow"]),
    );
    for key in CAPABILITIES {
        let value = effective_props.get(key).cloned();
        if value != config_prop(key) {
            sparse_leaf(&mut personal_props, key, value, inherited_prop(&[key]));
        }
    }
    let mut personal_input = object(at(personal, &["properties", "inputFormat"]));
    for key in ["supportsImage", "supportsVideo"] {
        let value = Some(json!(draft.input_flag(key)));
        if value != config_input(key) {
            sparse_leaf(
                &mut personal_input,
                key,
                value,
                inherited_prop(&["inputFormat", key]),
            );
        }
    }
    if personal_input.is_empty() {
        personal_props.remove("inputFormat");
    } else {
        personal_props.insert("inputFormat".into(), Value::Object(personal_input));
    }
    // 数值显式填写即是覆盖；即使与推荐相等也保留，清空输入才表示撤销覆盖。
    if draft.context_window.trim().is_empty() {
        personal_props.remove("contextWindow");
    } else {
        personal_props.insert("contextWindow".into(), json!(context_window));
    }
    for key in CAPABILITIES {
        if draft.overridden.contains(&format!("{key}Value")) {
            personal_props.insert(key.into(), effective_props[key].clone());
        }
    }
    for key in MODALITIES {
        let value = draft.input_flag(key);
        if draft
            .overridden
            .contains(&format!("inputFormatValue.{key}"))
            || Some(json!(value)) != config_input(key)
        {
            let mut input = object(personal_props.get("inputFormat"));
            input.insert(key.into(), json!(value));
            personal_props.insert("inputFormat".into(), Value::Object(input));
        }
    }

    let mut sparse = object(Some(personal));
    // 未触碰的启用状态保留现有 Overlay，不能因 Effective 默认值凭空写入（desktop resolvePersonalBoolean）。
    if draft.enabled != current_enabled {
        let inherited_enabled = inherited_at(&["enabled"]).and_then(Value::as_bool);
        if inherited_enabled != Some(draft.enabled) {
            sparse.insert("enabled".into(), json!(draft.enabled));
        }
    }
    if personal_props.is_empty() {
        sparse.remove("properties");
    } else {
        sparse.insert("properties".into(), Value::Object(personal_props));
    }

    // Option specs (desktop `applyPersonalReasoning` + max output).
    let mut personal_specs = object(at(personal, &["optionSpecs"]));
    let mut reasoning = object(at(personal, &["optionSpecs", "reasoningLevel"]));
    let current_levels = super::model_draft::levels(config);
    if !levels_equal(&levels, &current_levels) {
        let inherited_levels = inherited
            .map(super::model_draft::levels)
            .unwrap_or_default();
        if levels_equal(&levels, &inherited_levels) {
            reasoning.remove("values");
        } else {
            reasoning.insert("values".into(), json!(levels));
        }
    }
    if level_map.is_empty() || Some(level_map) == inherited_map {
        reasoning.remove("map");
    } else {
        reasoning.insert("map".into(), json!(level_map));
    }
    if draft.overridden.contains("reasoningLevelValuesValue") {
        reasoning.insert("values".into(), json!(levels));
    }
    if !level_map.is_empty() {
        reasoning.insert("map".into(), json!(level_map));
    }
    if reasoning.is_empty() {
        personal_specs.remove("reasoningLevel");
    } else {
        personal_specs.insert("reasoningLevel".into(), Value::Object(reasoning));
    }
    let personal_max_map = at(personal, &["optionSpecs", "maxOutputTokens", "map"]).cloned();
    let mut max_spec = Object::new();
    if let Some(map) = personal_max_map {
        max_spec.insert("map".into(), map);
    }
    if let Some(max) = max_output {
        max_spec.insert("max".into(), json!(max));
    }
    let max_output_map = draft.max_output_map.trim();
    if !max_output_map.is_empty() {
        max_spec.insert("map".into(), json!(max_output_map));
    }
    if max_spec.is_empty() {
        personal_specs.remove("maxOutputTokens");
    } else {
        personal_specs.insert("maxOutputTokens".into(), Value::Object(max_spec));
    }
    if personal_specs.is_empty() {
        sparse.remove("optionSpecs");
    } else {
        sparse.insert("optionSpecs".into(), Value::Object(personal_specs));
    }

    let config = if recommended {
        Value::Object(sparse)
    } else {
        materialize(
            &sparse,
            &effective_props,
            &levels,
            &effective_map,
            max_output,
            max_output_map,
        )
    };
    Ok(DraftCommit {
        model_id: model_id.to_owned(),
        use_recommended: recommended,
        config,
    })
}

/// Manual mode freezes only the editable leaves (desktop
/// `materializeEditorManagedPersonalConfig` → `extractManualModelConfig`).
fn materialize(
    sparse: &Object,
    props: &Object,
    levels: &[String],
    map: &str,
    max_output: Option<u64>,
    max_output_map: &str,
) -> Value {
    let input = &props["inputFormat"];
    let mut result = Object::new();
    // enabled 由模型列表行单独管理，不随固定/推荐模式物化或清除。
    if let Some(enabled) = sparse.get("enabled") {
        result.insert("enabled".into(), enabled.clone());
    }
    result.insert(
        "properties".into(),
        json!({
            "contextWindow": props["contextWindow"],
            "supportsJsonSchemaOutput": props["supportsJsonSchemaOutput"],
            "supportsNativeWebSearch": props["supportsNativeWebSearch"],
            "supportsMidConversationSystem": props["supportsMidConversationSystem"],
            "inputFormat": {
                "supportsImage": input["supportsImage"],
                "supportsVideo": input["supportsVideo"],
                "supportsPdf": input["supportsPdf"],
            },
        }),
    );
    let mut max_spec = json!({ "max": max_output });
    if !max_output_map.is_empty() {
        max_spec["map"] = json!(max_output_map);
    }
    result.insert(
        "optionSpecs".into(),
        json!({
            "reasoningLevel": { "values": levels, "map": map },
            "maxOutputTokens": max_spec,
        }),
    );
    Value::Object(result)
}

fn sparse_leaf(target: &mut Object, key: &str, value: Option<Value>, inherited: Option<Value>) {
    match value {
        Some(value) if Some(&value) != inherited.as_ref() => {
            target.insert(key.into(), value);
        }
        _ => {
            target.remove(key);
        }
    }
}

/// Desktop `arraysEqual`: two empty lists match, otherwise element-wise.
fn levels_equal(left: &[String], right: &[String]) -> bool {
    left == right
}
