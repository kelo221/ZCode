use crate::transcript::tool_card::{plan_result_status, tool_summary};
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

#[test]
fn approved_plan_result_keeps_only_its_status_line() {
    let approved = "User has approved your plan. You can now start coding. Start with updating your todo list if applicable.\n\n## Approved Plan:\n# Plan\n\n- step";
    assert_eq!(
        plan_result_status(approved),
        "User has approved your plan. You can now start coding. Start with updating your todo list if applicable."
    );
    let rejected = "User rejected the plan.\n\n## Notes\nkeep this";
    assert_eq!(plan_result_status(rejected), rejected);
}
