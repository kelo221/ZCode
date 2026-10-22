use super::*;

#[test]
fn agent_title_fallback() {
    assert_eq!(
        format_agent_title("Explore codebase", "task"),
        "Explore codebase"
    );
    assert_eq!(format_agent_title("   ", "task"), "task");
    assert_eq!(format_agent_title("", "subagent"), "subagent");
}

#[test]
fn ended_summary_formatting() {
    assert_eq!(format_ended_summary(0), None);
    assert_eq!(
        format_ended_summary(1),
        Some("1 ended subagent".to_string())
    );
    assert_eq!(
        format_ended_summary(5),
        Some("5 ended subagents".to_string())
    );
}
