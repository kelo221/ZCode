use super::*;
use serde_json::json;

#[test]
fn test_plugins_overview_parsing() {
    let v = json!({
        "marketplaces": [
            {
                "id": "zcode-plugins-official",
                "name": "Official Marketplace",
                "pluginCount": 10,
                "isOfficial": true,
                "featured": ["git-tools", "ai-flow"]
            }
        ],
        "availablePlugins": [
            {
                "id": "git-tools",
                "name": "Git Tools",
                "marketplace": "zcode-plugins-official",
                "description": "Helper for git commands",
                "installed": false,
                "listing": {
                    "displayName": "Git Tools Pro",
                    "category": "Development",
                    "examplePrompts": ["Review staged diffs"]
                }
            }
        ],
        "installedPlugins": [
            {
                "id": "linter",
                "name": "Linter",
                "marketplace": "zcode-plugins-official",
                "enabled": true,
                "scope": "workspace",
                "updateStatus": "update-available",
                "latestVersion": "1.2.0"
            }
        ],
        "restorableBuiltins": [],
        "capability": {
            "supported": true
        }
    });

    let overview = PluginsOverviewResult::from_value(&v);
    assert_eq!(overview.marketplaces.len(), 1);
    assert!(overview.marketplaces[0].is_official);
    assert_eq!(overview.marketplaces[0].featured.as_ref().unwrap().len(), 2);

    assert_eq!(overview.available_plugins.len(), 1);
    assert_eq!(
        overview.available_plugins[0].display_label(),
        "Git Tools Pro"
    );

    assert_eq!(overview.installed_plugins.len(), 1);
    assert!(overview.installed_plugins[0].enabled);
    assert_eq!(
        overview.installed_plugins[0].update_status.as_deref(),
        Some("update-available")
    );
}

#[test]
fn available_card_ids_are_unique_per_section_and_marketplace() {
    let ids = [
        available_card_id("pdf", "zcode-plugins-official", true),
        available_card_id("pdf", "zcode-plugins-official", false),
        available_card_id("pdf", "my-source", false),
    ];
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
}
