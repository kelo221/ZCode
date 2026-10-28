use super::*;

const TASK_OUTPUT: &str = "<retrieval_status>success</retrieval_status>\n\n\
<task_id>b7x2</task_id>\n\n<task_type>local_bash</task_type>\n\n<status>completed</status>\n\n\
<output>\nbuild ok &amp; 3 &lt;warnings&gt;\n</output>";

const NOTIFICATION: &str = "<task-notification>\n  <task-id>a1</task-id>\n  \
<task-type>local_agent</task-type>\n  <status>completed</status>\n  \
<description>Explore repo</description>\n  <summary>Agent finished</summary>\n  \
<result>Found 3 files</result>\n</task-notification>";

#[test]
fn task_output_is_parsed_and_unescaped() {
    let t = parse_task_output(TASK_OUTPUT).unwrap();
    assert_eq!(t.retrieval_status, "success");
    assert_eq!(t.task_id.as_deref(), Some("b7x2"));
    assert_eq!(t.status.as_deref(), Some("completed"));
    assert_eq!(t.output.as_deref(), Some("build ok & 3 <warnings>"));
    assert_eq!(clean_tool_output(TASK_OUTPUT), "build ok & 3 <warnings>");
}

#[test]
fn not_ready_task_output_has_no_output() {
    let t = parse_task_output("<retrieval_status>not_ready</retrieval_status>").unwrap();
    assert_eq!(t.output, None);
    assert_eq!(t.task_id, None);
}

#[test]
fn notification_prefers_result_then_error_then_summary() {
    let n = parse_task_notification(NOTIFICATION).unwrap();
    assert_eq!(n.task_id, "a1");
    assert_eq!(n.status, "completed");
    assert_eq!(n.description.as_deref(), Some("Explore repo"));
    assert_eq!(n.body(), "Found 3 files");
    let failed = parse_task_notification(
        "<task-notification><status>failed</status><summary>s</summary><error>boom</error></task-notification>",
    )
    .unwrap();
    assert_eq!(failed.body(), "boom");
}

#[test]
fn system_reminders_are_removed_everywhere() {
    let text =
        "result line\n<system-reminder>\nhidden\n</system-reminder>\ntail\n<system-reminder>open";
    assert_eq!(strip_system_reminders(text), "result line\n\ntail");
    assert_eq!(
        clean_tool_output("<system-reminder>x</system-reminder>"),
        ""
    );
}

#[test]
fn remaining_items_block_becomes_a_list() {
    let text = "Two items done.\n\n<remaining_items>\n- Enemy effects\n2. Master bugs\n\n</remaining_items>";
    assert_eq!(
        remaining_items_as_markdown(text),
        "Two items done.\n\n**Unfinished items**\n\n- Enemy effects\n- Master bugs"
    );
    // Unclosed at the end (still streaming, or the model never closed it).
    assert_eq!(
        remaining_items_as_markdown("Done.\n<remaining_items>\n* Item three"),
        "Done.\n\n**Unfinished items**\n\n- Item three"
    );
    // Text after the block stays a separate paragraph.
    assert_eq!(
        remaining_items_as_markdown("A\n<remaining_items>\n- x\n</remaining_items>\nTail"),
        "A\n\n**Unfinished items**\n\n- x\n\nTail"
    );
}

#[test]
fn empty_remaining_items_block_is_dropped() {
    assert_eq!(
        remaining_items_as_markdown("All done.\n<remaining_items>\n</remaining_items>"),
        "All done."
    );
}

#[test]
fn remaining_items_mentions_in_prose_or_code_are_untouched() {
    let prose = "The runner strips the `<remaining_items>` block.";
    assert_eq!(remaining_items_as_markdown(prose), prose);
    let fenced = "Example:\n```\n<remaining_items>\n- a\n</remaining_items>\n```";
    assert_eq!(remaining_items_as_markdown(fenced), fenced);
    assert!(matches!(
        remaining_items_as_markdown("plain"),
        Cow::Borrowed(_)
    ));
}

#[test]
fn plain_output_is_untouched() {
    assert_eq!(clean_tool_output("a < b and c > d"), "a < b and c > d");
}
