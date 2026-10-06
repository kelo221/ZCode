use serde_json::{Map, Value};
use std::collections::HashSet;

pub(crate) const ARTIFACT_ERROR: &str = "Workflow artifacts could not be read";
pub(crate) const ARTIFACT_UNAVAILABLE: &str = "Workflow artifact inspection is unavailable";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunArtifactMetadata {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub content_type: Option<String>,
    pub version: u64,
    pub item_count: u64,
    pub primary: bool,
}

pub(crate) fn parse_artifacts(value: &Value) -> Result<Vec<RunArtifactMetadata>, &'static str> {
    parse(value).ok_or(ARTIFACT_ERROR)
}

fn parse(value: &Value) -> Option<Vec<RunArtifactMetadata>> {
    let root = object(value, &["artifacts"])?;
    let rows = root.get("artifacts")?.as_array()?;
    if rows.len() > 256 {
        return None;
    }
    let mut ids = HashSet::new();
    let mut primary_seen = false;
    rows.iter()
        .map(|value| {
            let row = object(
                value,
                &[
                    "id",
                    "kind",
                    "title",
                    "description",
                    "contentType",
                    "sourcePath",
                    "spec",
                    "version",
                    "versions",
                    "itemCount",
                    "primary",
                ],
            )?;
            let id = string(row.get("id")?, 64)?;
            if id.trim().is_empty() || id.chars().any(char::is_control) || !ids.insert(id) {
                return None;
            }
            let kind = row.get("kind")?.as_str()?;
            if !matches!(
                kind,
                "file" | "markdown" | "chart" | "table" | "metrics" | "board"
            ) {
                return None;
            }
            let title = optional_string(row, "title", 120)?;
            optional_string(row, "description", 500)?;
            let content_type = optional_string(row, "contentType", 128)?;
            optional_string(row, "sourcePath", 1024)?;
            let version = version_number(row.get("version")?)?;
            let item_count = integer(row.get("itemCount")?)?;
            let primary = is_primary(row)?;
            if primary && primary_seen {
                return None;
            }
            primary_seen |= primary;
            let versions = row.get("versions")?.as_array()?;
            if versions.is_empty() || versions.len() > 16 {
                return None;
            }
            let mut last = 0;
            for value in versions {
                let record = object(
                    value,
                    &[
                        "version",
                        "title",
                        "description",
                        "contentType",
                        "bytes",
                        "uri",
                        "sourcePath",
                        "spec",
                        "publishedAt",
                        "primary",
                    ],
                )?;
                let next = version_number(record.get("version")?)?;
                if next <= last {
                    return None;
                }
                last = next;
                for (key, limit) in [
                    ("title", 120),
                    ("description", 500),
                    ("contentType", 128),
                    ("uri", 512),
                    ("sourcePath", 1024),
                ] {
                    optional_string(record, key, limit)?;
                }
                if let Some(bytes) = record.get("bytes") {
                    integer(bytes)?;
                }
                integer(record.get("publishedAt")?)?;
                is_primary(record)?;
            }
            if last != version {
                return None;
            }
            Some(RunArtifactMetadata {
                id: id.into(),
                kind: kind.into(),
                title: title.map(display_text),
                content_type: content_type.map(display_text),
                version,
                item_count,
                primary,
            })
        })
        .collect()
}

fn object<'a>(value: &'a Value, fields: &[&str]) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    object
        .keys()
        .all(|key| fields.contains(&key.as_str()))
        .then_some(object)
}

fn string(value: &Value, max: usize) -> Option<&str> {
    let string = value.as_str()?;
    (!string.is_empty() && string.encode_utf16().count() <= max).then_some(string)
}

fn optional_string<'a>(
    row: &'a Map<String, Value>,
    key: &str,
    max: usize,
) -> Option<Option<&'a str>> {
    match row.get(key) {
        Some(value) => string(value, max).map(Some),
        None => Some(None),
    }
}

fn integer(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n <= 9_007_199_254_740_991)
}

fn version_number(value: &Value) -> Option<u64> {
    integer(value).filter(|n| (1..=16).contains(n))
}

fn is_primary(row: &Map<String, Value>) -> Option<bool> {
    match row.get("primary") {
        None => Some(false),
        Some(Value::Bool(true)) => Some(true),
        _ => None,
    }
}

fn display_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(test)]
#[path = "workflow_artifacts_tests.rs"]
mod tests;
