use crate::transcript::tool_card::tool_summary;
use crate::transcript::tool_results::{displayed_result, is_todo_result_echo, plan_result_status};
use serde_json::json;

#[test]
fn todo_write_json_echo_is_hidden_but_errors_are_kept() {
    let echo = json!({
        "oldTodos": [{"content":"a","status":"pending","priority":"high"}],
        "todos": [{"content":"a","status":"completed","priority":"high"}],
        "summary": {"total":1,"pending":0,"inProgress":0,"completed":1}
    })
    .to_string();
    assert!(is_todo_result_echo(&echo));
    assert!(is_todo_result_echo(r#"{"todos":[]}"#));
    assert!(!is_todo_result_echo("Error: todos must be an array"));
    assert!(!is_todo_result_echo(r#"{"error":"invalid todos"}"#));
    assert!(!is_todo_result_echo(r#"{"todos":"not a list"}"#));
    assert_eq!(displayed_result("TodoWrite", echo.clone()), "");
    assert_eq!(
        displayed_result("TodoWrite", "Error: bad".into()),
        "Error: bad"
    );
    assert_eq!(displayed_result("Read", echo.clone()), echo);
}

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
