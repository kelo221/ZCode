use super::*;
use serde_json::json;

fn fixture() -> Value {
    json!({"artifacts":[{"id":"report","kind":"markdown","title":"Report","version":2,
        "versions":[{"version":1,"publishedAt":1,"uri":"zcode-artifact://session/private"},
            {"version":2,"publishedAt":2,"uri":"zcode-artifact://session/latest"}],
        "itemCount":0,"primary":true,"sourcePath":"private.md","spec":{"private":"payload"}}]})
}

#[test]
fn artifact_metadata_preserves_safe_fields_without_store_paths_or_specs() {
    let rows = parse_artifacts(&fixture()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "report");
    assert_eq!(rows[0].version, 2);
    assert!(rows[0].primary);
    let display = format!("{rows:?}");
    assert!(!display.contains("zcode-artifact"));
    assert!(!display.contains("private"));
    assert!(
        parse_artifacts(&json!({"artifacts":[]}))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn artifact_metadata_rejects_malformed_rows_versions_and_unknown_fields_as_a_whole() {
    for (key, value) in [
        ("kind", json!("future")),
        ("title", Value::Null),
        ("version", json!(17)),
        ("itemCount", json!(-1)),
        ("primary", json!(false)),
        ("unknown", json!(1)),
        ("versions", json!([])),
        ("id", json!(" ")),
    ] {
        let mut bad = fixture();
        bad["artifacts"][0][key] = value;
        assert_eq!(parse_artifacts(&bad).unwrap_err(), ARTIFACT_ERROR, "{key}");
    }
    let mut bad = fixture();
    bad["artifacts"][0]["versions"][1]["version"] = json!(1);
    assert!(parse_artifacts(&bad).is_err());
    bad = fixture();
    bad["artifacts"][0]["versions"][0]["uri"] = Value::Null;
    assert!(parse_artifacts(&bad).is_err());
    bad = fixture();
    let duplicate = bad["artifacts"][0].clone();
    bad["artifacts"].as_array_mut().unwrap().push(duplicate);
    assert!(parse_artifacts(&bad).is_err());
    let row = fixture()["artifacts"][0].clone();
    assert!(parse_artifacts(&json!({"artifacts":vec![row;257]})).is_err());
}
