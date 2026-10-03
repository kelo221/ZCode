//! Plugin store data structures and JSON parsing.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts (zcodePluginsOverviewResultSchema)
//! and CONTEXT.md.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginStoreListing {
    pub display_name: Option<String>,
    pub icon: Option<String>,
    pub category: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub homepage: Option<String>,
    pub privacy_policy: Option<String>,
    pub terms_of_service: Option<String>,
    pub hero_image: Option<String>,
    pub example_prompts: Option<Vec<String>>,
    pub requires_paid_plan: Option<bool>,
}

impl PluginStoreListing {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            display_name: v
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::to_string),
            icon: v.get("icon").and_then(Value::as_str).map(str::to_string),
            category: v
                .get("category")
                .and_then(Value::as_str)
                .map(str::to_string),
            author: v.get("author").and_then(Value::as_str).map(str::to_string),
            author_url: v
                .get("authorUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
            homepage: v
                .get("homepage")
                .and_then(Value::as_str)
                .map(str::to_string),
            privacy_policy: v
                .get("privacyPolicy")
                .and_then(Value::as_str)
                .map(str::to_string),
            terms_of_service: v
                .get("termsOfService")
                .and_then(Value::as_str)
                .map(str::to_string),
            hero_image: v
                .get("heroImage")
                .and_then(Value::as_str)
                .map(str::to_string),
            example_prompts: v
                .get("examplePrompts")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                }),
            requires_paid_plan: v.get("requiresPaidPlan").and_then(Value::as_bool),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginMarketplaceSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub last_updated: Option<String>,
    pub plugin_count: u64,
    pub is_official: bool,
    pub featured: Option<Vec<String>>,
}

impl PluginMarketplaceSummary {
    pub fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let description = v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        let last_updated = v
            .get("lastUpdated")
            .and_then(Value::as_str)
            .map(str::to_string);
        let plugin_count = v.get("pluginCount").and_then(Value::as_u64).unwrap_or(0);
        let is_official = v
            .get("isOfficial")
            .and_then(Value::as_bool)
            .unwrap_or(id == "zcode-plugins-official");
        let featured = v.get("featured").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });

        Some(Self {
            id,
            name,
            description,
            last_updated,
            plugin_count,
            is_official,
            featured,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AvailablePluginSummary {
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub installed: bool,
    pub component_types: Option<Vec<String>>,
    pub listing: Option<PluginStoreListing>,
}

impl AvailablePluginSummary {
    pub fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let marketplace = v
            .get("marketplace")
            .and_then(Value::as_str)
            .unwrap_or("zcode-plugins-official")
            .to_string();
        let description = v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        let version = v.get("version").and_then(Value::as_str).map(str::to_string);
        let installed = v.get("installed").and_then(Value::as_bool).unwrap_or(false);
        let component_types = v
            .get("componentTypes")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            });
        let listing = v.get("listing").and_then(PluginStoreListing::from_value);

        Some(Self {
            id,
            name,
            marketplace,
            description,
            version,
            installed,
            component_types,
            listing,
        })
    }

    pub fn display_label(&self) -> &str {
        if let Some(l) = &self.listing
            && let Some(name) = &l.display_name
        {
            return name;
        }
        &self.name
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstalledPluginSummary {
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub enabled: bool,
    pub scope: String,
    pub install_path: Option<String>,
    pub installed_at: Option<String>,
    pub component_types: Option<Vec<String>>,
    pub update_status: Option<String>,
    pub latest_version: Option<String>,
    pub listing: Option<PluginStoreListing>,
}

impl InstalledPluginSummary {
    pub fn from_value(v: &Value) -> Option<Self> {
        let id = v.get("id").and_then(Value::as_str)?.to_string();
        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let marketplace = v
            .get("marketplace")
            .and_then(Value::as_str)
            .unwrap_or("zcode-plugins-official")
            .to_string();
        let description = v
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        let version = v.get("version").and_then(Value::as_str).map(str::to_string);
        let enabled = v.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let scope = v
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("workspace")
            .to_string();
        let install_path = v
            .get("installPath")
            .and_then(Value::as_str)
            .map(str::to_string);
        let installed_at = v
            .get("installedAt")
            .and_then(Value::as_str)
            .map(str::to_string);
        let component_types = v
            .get("componentTypes")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            });
        let update_status = v
            .get("updateStatus")
            .and_then(Value::as_str)
            .map(str::to_string);
        let latest_version = v
            .get("latestVersion")
            .and_then(Value::as_str)
            .map(str::to_string);
        let listing = v.get("listing").and_then(PluginStoreListing::from_value);

        Some(Self {
            id,
            name,
            marketplace,
            description,
            version,
            enabled,
            scope,
            install_path,
            installed_at,
            component_types,
            update_status,
            latest_version,
            listing,
        })
    }

    pub fn display_label(&self) -> &str {
        if let Some(l) = &self.listing
            && let Some(name) = &l.display_name
        {
            return name;
        }
        &self.name
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginsOverviewResult {
    pub marketplaces: Vec<PluginMarketplaceSummary>,
    pub available_plugins: Vec<AvailablePluginSummary>,
    pub installed_plugins: Vec<InstalledPluginSummary>,
    pub restorable_builtins: Vec<AvailablePluginSummary>,
    pub capability_supported: bool,
}

impl PluginsOverviewResult {
    pub fn from_value(v: &Value) -> Self {
        let marketplaces = v
            .get("marketplaces")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(PluginMarketplaceSummary::from_value)
                    .collect()
            })
            .unwrap_or_default();

        let available_plugins = v
            .get("availablePlugins")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(AvailablePluginSummary::from_value)
                    .collect()
            })
            .unwrap_or_default();

        let installed_plugins = v
            .get("installedPlugins")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(InstalledPluginSummary::from_value)
                    .collect()
            })
            .unwrap_or_default();

        let restorable_builtins = v
            .get("restorableBuiltins")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(AvailablePluginSummary::from_value)
                    .collect()
            })
            .unwrap_or_default();

        let capability_supported = v
            .get("capability")
            .and_then(|c| c.get("supported"))
            .and_then(Value::as_bool)
            .unwrap_or(true);

        Self {
            marketplaces,
            available_plugins,
            installed_plugins,
            restorable_builtins,
            capability_supported,
        }
    }
}

/// Element id of an available-plugin card. A featured plugin is rendered in
/// both the Featured and the All lists, and names repeat across
/// marketplaces, so the id carries the section, marketplace and plugin id;
/// ids inside the card are scoped under it.
pub(crate) fn available_card_id(plugin_id: &str, marketplace: &str, featured: bool) -> String {
    let section = if featured { "featured" } else { "all" };
    format!("plugin-card/{section}/{marketplace}/{plugin_id}")
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
