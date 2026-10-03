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
fn summary_truncation() {
    assert_eq!(truncate_summary(None, 10), None);
    assert_eq!(truncate_summary(Some("   "), 10), None);
    assert_eq!(
        truncate_summary(Some("Short"), 10),
        Some("Short".to_string())
    );
    assert_eq!(
        truncate_summary(Some("This is a long summary text that exceeds limit"), 15),
        Some("This is a long ...".to_string())
    );
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
