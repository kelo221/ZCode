//! `toolSearchMode` in the shared `setting.json` (PARITY.md "Load MCP tools on demand").

use crate::shared::settings::AppSettings;

/// Values Desktop accepts (`appToolSearchModeSchema`); anything else means `off`.
pub(crate) const TOOL_SEARCH_MODES: [&str; 3] = ["auto", "local", "off"];

pub(crate) fn parse_tool_search_mode(value: Option<&str>) -> &'static str {
    value
        .and_then(|v| TOOL_SEARCH_MODES.iter().find(|m| **m == v))
        .copied()
        .unwrap_or("off")
}

impl AppSettings {
    pub(crate) fn tool_search_mode(&self) -> &'static str {
        parse_tool_search_mode(self.tool_search_mode.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_missing_modes_mean_off() {
        assert_eq!(parse_tool_search_mode(Some("auto")), "auto");
        assert_eq!(parse_tool_search_mode(Some("local")), "local");
        assert_eq!(parse_tool_search_mode(Some("native")), "off");
        assert_eq!(parse_tool_search_mode(None), "off");
    }

    #[test]
    fn the_shared_key_round_trips_and_is_omitted_when_unset() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"toolSearchMode":"local","other":1}"#).unwrap();
        assert_eq!(settings.tool_search_mode(), "local");
        let text = serde_json::to_string(&settings).unwrap();
        assert!(text.contains(r#""toolSearchMode":"local""#), "{text}");
        assert!(text.contains(r#""other":1"#), "{text}");
        let empty = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(!empty.contains("toolSearchMode"), "{empty}");
    }
}
