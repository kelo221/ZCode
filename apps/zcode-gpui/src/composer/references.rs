use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogKind {
    Skills,
    Plugins,
}
impl CatalogKind {
    pub fn method(self) -> &'static str {
        match self {
            Self::Skills => "skills/referenceCatalog",
            Self::Plugins => "plugins/referenceCatalog",
        }
    }
}

#[derive(Default)]
pub(crate) enum CatalogState {
    #[default]
    Idle,
    Loading,
    Ready(Vec<ReferenceEntry>),
    Failed(String),
}

#[derive(Default)]
pub(crate) struct ReferenceCatalog {
    pub skills: CatalogState,
    pub plugins: CatalogState,
}
impl ReferenceCatalog {
    pub fn state(&self, kind: CatalogKind) -> &CatalogState {
        match kind {
            CatalogKind::Skills => &self.skills,
            CatalogKind::Plugins => &self.plugins,
        }
    }
    pub fn state_mut(&mut self, kind: CatalogKind) -> &mut CatalogState {
        match kind {
            CatalogKind::Skills => &mut self.skills,
            CatalogKind::Plugins => &mut self.plugins,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceEntry {
    pub category: &'static str,
    pub name: String,
    pub description: String,
    pub insert_text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillsResult {
    authority: String,
    skills: Vec<Skill>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Skill {
    id: String,
    name: String,
    description: String,
    path: String,
    scope: String,
    enabled: bool,
    plugin_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginsResult {
    authority: String,
    plugins: Vec<Plugin>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Plugin {
    category: Option<String>,
    plugin_id: String,
    name: String,
    marketplace: String,
    icon: Option<String>,
    display_name: Option<String>,
    display_name_i18n: Option<std::collections::HashMap<String, String>>,
    description: Option<String>,
    description_i18n: Option<std::collections::HashMap<String, String>>,
    enabled: bool,
    conflicting_plugin_ids: Vec<String>,
    skill_qualified_names: Vec<String>,
    mcp_server_names: Vec<String>,
    #[serde(default)]
    subagent_names: Vec<String>,
}

pub(crate) fn parse_catalog(
    value: Value,
    kind: CatalogKind,
    session: bool,
) -> Result<Vec<ReferenceEntry>, String> {
    let expected = if session { "session" } else { "workspace" };
    let invalid = || "Invalid reference catalog response".to_string();
    match kind {
        CatalogKind::Skills => {
            let result: SkillsResult = serde_json::from_value(value).map_err(|_| invalid())?;
            if result.authority != expected {
                return Err("Reference catalog authority mismatch".into());
            }
            result
                .skills
                .into_iter()
                .map(|skill| {
                    if skill.id.trim().is_empty()
                        || skill.name.trim().is_empty()
                        || skill.path.trim().is_empty()
                        || !skill.enabled
                        || !matches!(skill.scope.as_str(), "workspace" | "user" | "plugin")
                    {
                        return Err(invalid());
                    }
                    let _ = skill.plugin_name;
                    Ok(ReferenceEntry {
                        category: "skill",
                        insert_text: mention_markdown(&format!("${}", skill.name), &skill.path),
                        name: format!("${}", skill.name),
                        description: skill.description,
                    })
                })
                .collect()
        }
        CatalogKind::Plugins => {
            let result: PluginsResult = serde_json::from_value(value).map_err(|_| invalid())?;
            if result.authority != expected {
                return Err("Reference catalog authority mismatch".into());
            }
            let mut entries = vec![];
            for plugin in result.plugins {
                if plugin.plugin_id.trim().is_empty()
                    || plugin.name.trim().is_empty()
                    || plugin.marketplace.trim().is_empty()
                {
                    return Err(invalid());
                }
                if !plugin.enabled || !plugin.conflicting_plugin_ids.is_empty() {
                    continue;
                }
                let locale = crate::shared::i18n::current_locale().as_str();
                let description = plugin
                    .description_i18n
                    .as_ref()
                    .and_then(|map| map.get(locale))
                    .cloned()
                    .or(plugin.description)
                    .unwrap_or_default();
                let _ = (
                    plugin.category,
                    plugin.icon,
                    plugin.display_name,
                    plugin.display_name_i18n,
                    plugin.skill_qualified_names,
                    plugin.mcp_server_names,
                    plugin.subagent_names,
                );
                entries.push(ReferenceEntry {
                    category: "plugin",
                    insert_text: mention_markdown(
                        &format!("@{}", plugin.name),
                        &format!("plugin://{}", plugin.plugin_id),
                    ),
                    name: format!("@{}", plugin.name),
                    description,
                });
            }
            Ok(entries)
        }
    }
}

fn mention_markdown(label: &str, destination: &str) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let has_scheme = destination.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    let destination = if destination.starts_with(['/', '#'])
        || destination.starts_with("./")
        || destination.starts_with("../")
        || has_scheme
    {
        destination.to_string()
    } else {
        format!("./{destination}")
    };
    format!(
        "[{label}]({})",
        destination.replace('\\', "\\\\").replace('>', "\\>")
    )
}

pub(crate) fn reference_query(text: &str) -> Option<(CatalogKind, &str, usize)> {
    let (idx, marker) = text
        .char_indices()
        .rev()
        .find(|(_, c)| matches!(c, '@' | '$'))?;
    if idx > 0 && !text[..idx].ends_with(char::is_whitespace) {
        return None;
    }
    let query = &text[idx + 1..];
    if query.chars().any(char::is_whitespace) {
        return None;
    }
    Some((
        if marker == '$' {
            CatalogKind::Skills
        } else {
            CatalogKind::Plugins
        },
        query,
        idx,
    ))
}

#[cfg(test)]
#[path = "references_tests.rs"]
mod tests;
