//! Slash commands catalog, filtering, and execution mapping.

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
    pub source: Option<SlashSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlashSource {
    Builtin,
    Custom,
}

#[cfg(test)]
pub fn builtin_slash_commands() -> Vec<SlashCommand> {
    ["compact", "goal", "init", "help"]
        .into_iter()
        .map(|name| SlashCommand {
            name: name.into(),
            description: format!("Test {name}"),
            input_hint: None,
            source: Some(SlashSource::Builtin),
        })
        .collect()
}

/// Matches slash commands against an input string starting with `/`.
/// Returns `None` if the input is not a slash command query (e.g. contains space or no `/`).
pub fn filter_slash_commands<'a>(
    catalog: &'a [SlashCommand],
    input: &str,
) -> Option<Vec<&'a SlashCommand>> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    let query_part = &trimmed[1..];
    // If user typed a space, they are typing arguments; close autocomplete.
    if query_part.contains(char::is_whitespace) {
        return None;
    }
    let q = query_part.to_lowercase();
    let matches: Vec<&'a SlashCommand> = catalog
        .iter()
        .filter(|cmd| cmd.name.to_lowercase().starts_with(&q))
        .collect();
    Some(matches)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SlashAction {
    Compact,
    Goal(String),
    ResumeGoal,
    Plan(String),
    Plain(String),
}

/// Classify command submission: whether it maps to a specialized V4 command
/// or remains normal text.
pub fn classify_slash_command(text: &str) -> SlashAction {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return SlashAction::Plain(text.to_string());
    }
    let without_slash = &trimmed[1..];
    let mut parts = without_slash.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("").to_lowercase();
    let rest = parts.next().unwrap_or("").trim();

    match cmd.as_str() {
        "plan" => SlashAction::Plan(rest.to_string()),
        "compact" | "compress" => SlashAction::Compact,
        "goal" | "target" if rest.eq_ignore_ascii_case("resume") => SlashAction::ResumeGoal,
        "goal" | "target" => SlashAction::Goal(rest.to_string()),
        _ => SlashAction::Plain(text.to_string()),
    }
}

#[cfg(test)]
#[path = "slash_tests.rs"]
mod tests;
