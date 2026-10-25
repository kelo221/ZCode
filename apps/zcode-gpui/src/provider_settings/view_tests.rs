use super::*;
use serde_json::json;

pub(crate) fn sample_view() -> Value {
    json!({
        "revision": 7,
        "providerTemplates": [
            { "templateId": "openrouter", "templateNameMap": { "en-US": "OpenRouter", "zh-CN": "OpenRouter 中文" }, "config": {} },
            { "templateId": "bare", "templateNameMap": {}, "config": {} }
        ],
        "providerOrder": ["c2", "c1"],
        "providers": [
            {
                "providerId": "c1", "providerName": "First", "enabled": true, "executable": true,
                "effectiveConfig": {
                    "group": "standard-personal",
                    "access": { "type": "api-key", "apiKey": "sk-1" },
                    "api": { "type": "openai-responses", "baseUrl": "https://one.test" }
                },
                "issues": [],
                "models": [{
                    "kind": "candidate", "modelId": "m1", "builtin": false, "enabled": true, "executable": true,
                    "effectiveConfig": { "properties": { "contextWindow": 200000, "inputFormat": { "supportsImage": true } } },
                    "issues": []
                }]
            },
            {
                "providerId": "zai", "providerName": "Z.ai", "enabled": true, "executable": false,
                "effectiveConfig": { "group": "zai-family", "access": { "type": "zhipu-account" } },
                "issues": [{ "code": "required-field-missing", "path": [], "message": "missing key" }],
                "models": []
            },
            {
                "providerId": "c2", "providerName": "Second", "enabled": false, "executable": true,
                "effectiveConfig": { "group": "standard-personal", "access": { "type": "api-key" } },
                "issues": [], "models": []
            }
        ]
    })
}

#[test]
fn parses_providers_models_and_templates() {
    let view = ProviderView::from_value(&sample_view()).unwrap();
    assert_eq!(view.revision, 7);
    let first = view.provider("c1").unwrap();
    assert!(first.custom() && first.uses_api_key() && !first.account());
    assert_eq!(
        (
            first.api_key.as_str(),
            first.api_type.as_str(),
            first.base_url.as_str()
        ),
        ("sk-1", "openai-responses", "https://one.test")
    );
    assert_eq!(first.models[0].context_window, Some(200_000));
    assert!(first.models[0].vision);
    let zai = view.provider("zai").unwrap();
    assert!(zai.account() && !zai.custom());
    assert_eq!(zai.issues, vec!["missing key".to_owned()]);
    // Missing api type falls back to the desktop default.
    assert_eq!(view.provider("c2").unwrap().api_type, "anthropic-messages");
    assert_eq!(view.templates[0].name_zh, "OpenRouter 中文");
    assert_eq!(view.templates[1].name_en, "bare");
}

#[test]
fn lists_presets_first_then_customs_in_provider_order() {
    let view = ProviderView::from_value(&sample_view()).unwrap();
    let ids: Vec<&str> = view.listed().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["zai", "c2", "c1"]);
    assert_eq!(view.moved_custom_order("c1", true).unwrap(), ["c1", "c2"]);
    assert!(view.moved_custom_order("c2", true).is_none());
}

#[test]
fn status_reflects_enabled_executable_and_issues() {
    let view = ProviderView::from_value(&sample_view()).unwrap();
    assert_eq!(view.provider("c1").unwrap().status(), Status::Ready);
    assert_eq!(view.provider("zai").unwrap().status(), Status::Unavailable);
    assert_eq!(view.provider("c2").unwrap().status(), Status::Disabled);
}

#[test]
fn compacts_token_counts() {
    assert_eq!(compact_tokens(200_000), "200K");
    assert_eq!(compact_tokens(128_000), "128K");
    assert_eq!(compact_tokens(1_000_000), "1M");
    assert_eq!(compact_tokens(1_500_000), "1.5M");
    assert_eq!(compact_tokens(32_768), "32.8K");
    assert_eq!(compact_tokens(512), "512");
}

#[test]
fn selection_falls_back_to_first_listed_provider() {
    let mut state = ProviderSettingsState {
        view: ProviderView::from_value(&sample_view()),
        ..Default::default()
    };
    assert_eq!(state.selected_provider().unwrap().id, "zai");
    state.selected = Some("c1".into());
    assert_eq!(state.selected_provider().unwrap().id, "c1");
    state.selected = Some("gone".into());
    assert_eq!(state.selected_provider().unwrap().id, "zai");
}
