use super::*;
use serde_json::{Value, json};

pub(super) fn raw_view() -> Value {
    json!({"revision":7,"providers":[{"providerId":"fixture","providerName":"Fixture Provider",
        "templateId":"fixture-template","config":{"api":{"type":"anthropic-messages",
            "baseUrl":"https://example.invalid","headers":{"fixture":"synthetic-secret"}},
            "access":{"type":"api-key","apiKey":"synthetic-secret"},"visibility":"visible"},
        "models":[{"modelId":"old","config":{"enabled":true,
            "optionSpecs":{"reasoningLevel":{"values":["old-low","old-high"],"map":"synthetic-map"},
                "maxOutputTokens":{"max":100,"map":"synthetic-map"}},"properties":{}}},
            {"modelId":"new","config":{"enabled":true,"properties":{},
                "optionSpecs":{"reasoningLevel":{"values":["new-low","new-high"],"map":"synthetic-map"},
                    "maxOutputTokens":{"max":100,"map":"synthetic-map"}}}}]}],
        "preferredSelection":{"providerId":"fixture","modelId":"old","options":{"reasoningLevel":"old-low"}},
        "effectiveSelection":null,"selectionIssue":"selection-missing"})
}

pub(super) fn view() -> ModelSelectionView {
    serde_json::from_value(raw_view()).unwrap()
}

#[test]
fn facade_nested_shape_decodes_to_safe_catalog_not_legacy_model_list() {
    let view = view();
    assert_eq!(view.revision, 7);
    assert_eq!(view.providers[0].provider_id, "fixture");
    assert_eq!(
        view.providers[0].models[1].reasoning_levels,
        ["new-low", "new-high"]
    );
    assert_eq!(view.preferred_selection.unwrap().model_id, "old");
    let serialized = serde_json::to_string(&self::view()).unwrap();
    let debug = format!("{:?}", self::view());
    for text in [serialized, debug] {
        for secret in [
            "synthetic-secret",
            "example.invalid",
            "apiKey",
            "headers",
            "synthetic-map",
        ] {
            assert!(!text.contains(secret), "{secret}");
        }
    }
    assert!(serde_json::from_value::<ModelSelectionView>(json!({"models":["old"]})).is_err());
}

#[test]
fn missing_facade_essentials_or_reasoning_values_fail_instead_of_empty_success() {
    for field in ["revision", "providers"] {
        let mut raw = raw_view();
        raw.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ModelSelectionView>(raw).is_err(),
            "{field}"
        );
    }
    for field in ["providerId", "models", "config"] {
        let mut raw = raw_view();
        raw["providers"][0].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ModelSelectionView>(raw).is_err(),
            "{field}"
        );
    }
    for field in ["modelId", "config"] {
        let mut raw = raw_view();
        raw["providers"][0]["models"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<ModelSelectionView>(raw).is_err(),
            "{field}"
        );
    }
    for values in [json!([]), json!([""]), json!(["low", "low"]), json!([1])] {
        let mut raw = raw_view();
        raw["providers"][0]["models"][0]["config"]["optionSpecs"]["reasoningLevel"]["values"] =
            values;
        assert!(serde_json::from_value::<ModelSelectionView>(raw).is_err());
    }
    let mut raw = raw_view();
    raw["providers"][0]["models"][0]["config"]
        .as_object_mut()
        .unwrap()
        .remove("optionSpecs");
    assert!(serde_json::from_value::<ModelSelectionView>(raw).is_err());
}

#[test]
fn model_changes_complete_new_reasoning_without_old_carry_and_clear_everything() {
    let view = view();
    let mut draft = FormDraft::new();
    draft.select_model(&view, "fixture", "old").unwrap();
    draft.set_reasoning(&view, Some("old-low")).unwrap();
    draft.select_model(&view, "fixture", "new").unwrap();
    assert_eq!(
        draft
            .fields
            .model_selection
            .as_ref()
            .unwrap()
            .reasoning_level(),
        Some("new-high")
    );
    draft.select_model(&view, "fixture", "new").unwrap();
    draft.set_reasoning(&view, Some("new-low")).unwrap();
    draft.select_model(&view, "fixture", "new").unwrap();
    assert_eq!(
        draft
            .fields
            .model_selection
            .as_ref()
            .unwrap()
            .reasoning_level(),
        Some("new-low")
    );
    assert!(draft.select_model(&view, "fixture", "missing").is_err());
    assert_eq!(
        draft.fields.model_selection.as_ref().unwrap().model_id,
        "new"
    );
    assert!(draft.set_reasoning(&view, Some("old-high")).is_err());
    draft.set_reasoning(&view, None).unwrap();
    assert!(
        draft
            .fields
            .model_selection
            .as_ref()
            .unwrap()
            .options
            .is_none()
    );
    draft.clear_model();
    assert!(draft.fields.model_selection.is_none());
}

#[test]
fn override_model_changes_use_new_levels_and_clear_complete_payload() {
    let agent: AgentSummary = serde_json::from_value(json!({"id":"plugin:fixture@store:helper",
        "name":"fixture:helper","description":"Synthetic","systemPrompt":"Prompt",
        "path":"fixture/helper.md","scope":"user","source":"plugin","enabled":true,
        "modelSelectionOverride":{"providerId":"fixture","modelId":"old",
            "options":{"reasoningLevel":"old-low"}}}))
    .unwrap();
    let mut draft = OverrideDraft::from_agent(&agent).unwrap();
    draft.select_model(&view(), "fixture", "new").unwrap();
    assert_eq!(
        draft.selection.as_ref().unwrap().reasoning_level(),
        Some("new-high")
    );
    draft.set_reasoning(&view(), Some("new-low")).unwrap();
    let OverridePayload::Plugin(payload) = draft.payload() else {
        panic!("plugin payload")
    };
    assert_eq!(
        payload.model_selection.unwrap().reasoning_level(),
        Some("new-low")
    );
    draft.clear();
    let OverridePayload::Plugin(payload) = draft.payload() else {
        panic!("plugin payload")
    };
    assert!(
        serde_json::to_value(payload)
            .unwrap()
            .get("modelSelection")
            .is_none()
    );
}

#[test]
fn historical_selection_is_not_automatically_completed_or_replaced() {
    let view = view();
    let selection: ModelSelection =
        serde_json::from_value(json!({"providerId":"fixture","modelId":"old"})).unwrap();
    assert_eq!(
        view.validate_selection(&selection),
        Err(SelectionError::ReasoningMissing)
    );
    assert!(selection.options.is_none());
    let mut invalid = selection.clone();
    invalid.options = Some(SelectionOptions {
        reasoning_level: Some("gone".into()),
    });
    assert_eq!(
        view.validate_selection(&invalid),
        Err(SelectionError::ReasoningUnsupported)
    );
}

#[test]
fn model_choices_are_provider_grouped_searchable_and_require_supported_api() {
    let mut raw = raw_view();
    let mut no_api = raw["providers"][0].clone();
    no_api["providerId"] = json!("no-api");
    no_api["config"] = json!({});
    raw["providers"].as_array_mut().unwrap().push(no_api);
    let view: ModelSelectionView = serde_json::from_value(raw).unwrap();
    let groups = model_choices(&view, "");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].choices.len(), 2);
    assert_eq!(groups[0].label, "Fixture Provider");
    assert_eq!(model_choices(&view, " NEW ")[0].choices.len(), 1);
    assert_eq!(model_choices(&view, "fixture provider")[0].choices.len(), 2);
    assert!(model_choices(&view, "no-match").is_empty());
}

#[test]
fn model_selection_schema_trims_and_rejects_unknown_options() {
    let selection: ModelSelection = serde_json::from_value(json!({"providerId":" fixture ",
        "modelId":" old ","options":{"reasoningLevel":" old-low "}}))
    .unwrap();
    assert_eq!(selection.provider_id, "fixture");
    assert_eq!(selection.model_id, "old");
    assert_eq!(selection.reasoning_level(), Some("old-low"));
    let bom: ModelSelection =
        serde_json::from_value(json!({"providerId":"\u{feff}fixture\u{feff}",
        "modelId":"old"}))
        .unwrap();
    assert_eq!(bom.provider_id, "fixture");
    for raw in [
        json!({"providerId":"fixture","modelId":"old","options":{"reasoningLevel":" "}}),
        json!({"providerId":"fixture","modelId":"old","options":{"reasoningLevel":null}}),
        json!({"providerId":"fixture","modelId":"old","options":{"future":true}}),
    ] {
        assert!(serde_json::from_value::<ModelSelection>(raw).is_err());
    }
}
