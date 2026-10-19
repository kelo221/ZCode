use crate::transcript::tool_card::tool_summary;
use serde_json::json;

#[test]
fn code_search_tools_summarise_their_query() {
    assert_eq!(
        tool_summary("FindFiles", &json!({ "query": "noise_free" })).as_deref(),
        Some("noise_free")
    );
    assert_eq!(
        tool_summary("SearchCode", &json!({ "query": "fn render" })).as_deref(),
        Some("\"fn render\"")
    );
    assert_eq!(
        tool_summary(
            "SearchCode",
            &json!({ "patterns": ["voiceEnabled", "codeSearchEnabled"] })
        )
        .as_deref(),
        Some("voiceEnabled | codeSearchEnabled")
    );
}
