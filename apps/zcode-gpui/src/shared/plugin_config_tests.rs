use super::*;
use serde_json::json;

pub(crate) fn fixture() -> Value {
    json!({"plugins":[{"id":"fixture@local","name":"fixture","marketplace":"local","enabled":true,"source":"local","skillRootCount":0,"commandRootCount":0,"mcpServerNames":[],"rootPath":"C:/fixtures/plugin","userConfig":{"token":{"type":"string","sensitive":true},"count":{"type":"number","default":3},"active":{"type":"boolean"}},"configuredOptions":{"count":4},"optionSources":{"token":"user","count":"user"}}],"diagnostics":[]})
}

#[test]
fn scoped_config_projection_rejects_malformed_and_secret_readback() {
    let value = fixture();
    let parsed = PluginConfigList::parse(&value).unwrap();
    assert_eq!(parsed.plugins[0].effective("count"), json!(4));
    assert_eq!(parsed.plugins[0].effective("active"), json!(false));
    assert_eq!(parsed.plugins[0].effective("token"), json!(""));
    let mut malformed = value.clone();
    malformed["plugins"][0]["configuredOptions"]["token"] = json!("never-show");
    let error = PluginConfigList::parse(&malformed).err().unwrap();
    assert!(!error.contains("never-show"));
    for (field, bad) in [
        ("enabledSource", json!("default")),
        ("userConfig", Value::Null),
        ("configuredOptions", json!({"count":[]})),
    ] {
        let mut malformed = value.clone();
        malformed["plugins"][0][field] = bad;
        assert!(PluginConfigList::parse(&malformed).is_err());
    }
    let mut malformed = value;
    malformed["diagnostics"] =
        json!([{"code":"error","message":"token=private","severity":"error"}]);
    assert!(
        !PluginConfigList::parse(&malformed)
            .err()
            .unwrap()
            .contains("private")
    );
}

#[test]
fn patches_only_include_edits_with_explicit_disjoint_clear_keys() {
    let parsed = PluginConfigList::parse(&fixture()).unwrap();
    let plugin = &parsed.plugins[0];
    let mut edits = BTreeMap::new();
    edits.insert("token".into(), ConfigEdit::Text(String::new()));
    assert!(plugin.patch(&edits).unwrap().is_empty());
    edits.insert("count".into(), ConfigEdit::Text("5".into()));
    edits.insert("active".into(), ConfigEdit::Bool(true));
    edits.insert("token".into(), ConfigEdit::Clear);
    let patch = plugin.patch(&edits).unwrap();
    assert_eq!(patch.options, json!({"count":5.0,"active":true}));
    assert_eq!(patch.clear_keys, vec!["token"]);
    let mut sensitive_bool = fixture();
    sensitive_bool["plugins"][0]["userConfig"]["active"]["sensitive"] = json!(true);
    let sensitive_bool = PluginConfigList::parse(&sensitive_bool).unwrap();
    let secret_edit = BTreeMap::from([("active".into(), ConfigEdit::Text("true".into()))]);
    assert_eq!(
        sensitive_bool.plugins[0]
            .patch(&secret_edit)
            .unwrap()
            .options,
        json!({"active":true})
    );
    edits.insert("count".into(), ConfigEdit::Text("NaN".into()));
    assert!(plugin.patch(&edits).is_err());
    edits.insert("count".into(), ConfigEdit::Text("".into()));
    assert!(plugin.patch(&edits).is_err());
    edits.insert("unknown".into(), ConfigEdit::Clear);
    assert!(plugin.patch(&edits).is_err());
}

#[test]
fn plugin_mutation_receipt_requires_identity_and_safe_diagnostics() {
    assert!(
        validate_mutation(
            &json!({"pluginId":"other","diagnostics":[]}),
            "fixture@local"
        )
        .is_err()
    );
    assert!(
        validate_mutation(
            &json!({"pluginId":"fixture@local","diagnostics":null}),
            "fixture@local"
        )
        .is_err()
    );
    let error = validate_mutation(&json!({"pluginId":"fixture@local","diagnostics":[{"code":"bad","message":"token=private","severity":"error"}]}), "fixture@local").unwrap_err();
    assert!(!error.contains("private"));
    let error = validate_mutation(&json!({"pluginId":"fixture@local","diagnostics":[{"code":"bad","message":"unlabelled-secret-sentinel","severity":"error"}]}), "fixture@local").unwrap_err();
    assert!(!error.contains("unlabelled-secret-sentinel"));
    assert!(validate_mutation(&json!({"pluginId":"fixture@local","diagnostics":[{"code":"bad","message":"bad","severity":null}]}), "fixture@local").is_err());
}
