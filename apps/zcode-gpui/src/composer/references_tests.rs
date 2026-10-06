use super::*;
use serde_json::json;

#[test]
fn catalog_is_strict_and_session_authority_cannot_fall_back() {
    let result = json!({"authority":"session", "skills":[{"id":"skill-id","name":"custom","description":"Custom skill","path":"/fixtures/SKILL.md","scope":"workspace","enabled":true}]});
    let entries = parse_catalog(result.clone(), CatalogKind::Skills, true).unwrap();
    assert_eq!(entries[0].insert_text, "[$custom](/fixtures/SKILL.md)");
    assert!(parse_catalog(result.clone(), CatalogKind::Skills, false).is_err());
    let mut invalid = result;
    invalid["skills"][0]["extra"] = json!(true);
    assert!(parse_catalog(invalid, CatalogKind::Skills, true).is_err());
}

#[test]
fn disabled_and_conflicting_plugins_are_not_insertable() {
    let entry = json!({"pluginId":"market/custom","name":"custom","marketplace":"market","enabled":true,"conflictingPluginIds":[],"skillQualifiedNames":[],"mcpServerNames":[]});
    let mut disabled = entry.clone();
    disabled["enabled"] = json!(false);
    let mut conflict = entry.clone();
    conflict["conflictingPluginIds"] = json!(["other/custom"]);
    let result = json!({"authority":"workspace","plugins":[entry, disabled, conflict]});
    let entries = parse_catalog(result, CatalogKind::Plugins, false).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].insert_text, "[@custom](plugin://market/custom)");
}

#[test]
fn markers_use_word_boundary_and_preserve_unicode_offsets() {
    assert_eq!(
        reference_query("Use $custom"),
        Some((CatalogKind::Skills, "custom", 4))
    );
    assert_eq!(
        reference_query("用 @custom"),
        Some((CatalogKind::Plugins, "custom", 4))
    );
    assert!(reference_query("person@example.com").is_none());
    assert!(reference_query("$custom done").is_none());
    assert!(reference_query("[@custom](plugin://market/custom)").is_none());
}
