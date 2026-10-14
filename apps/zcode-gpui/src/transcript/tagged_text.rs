//! Model-facing XML-ish wrappers the CLI puts into tool results and
//! background-result messages, turned into user-facing text.
//!
//! Formats: TaskOutput (`core/src/tool/handlers/task-output.ts`:
//! `<retrieval_status>`, `<task_id>`, `<status>`, `<output>`), task
//! notifications (`core/src/runtime-task/notification.ts`) and
//! `<system-reminder>` blocks, which Desktop never shows
//! (`conversation-message-projection-policy.ts`).

/// Inner text of the first `<tag>…</tag>`, XML-unescaped and trimmed.
pub fn tag_value(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&close)?;
    Some(unescape(text[start..end].trim()))
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Drop every `<system-reminder>…</system-reminder>` block (an unclosed one
/// runs to the end).
pub fn strip_system_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        out.push_str(&rest[..i]);
        rest = match rest[i..].find(CLOSE) {
            Some(j) => &rest[i + j + CLOSE.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskOutputText {
    pub retrieval_status: String,
    pub task_id: Option<String>,
    pub status: Option<String>,
    pub output: Option<String>,
}

pub fn parse_task_output(text: &str) -> Option<TaskOutputText> {
    Some(TaskOutputText {
        retrieval_status: tag_value(text, "retrieval_status")?,
        task_id: tag_value(text, "task_id"),
        status: tag_value(text, "status"),
        output: tag_value(text, "output").filter(|o| !o.is_empty()),
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskNotification {
    pub task_id: String,
    pub status: String,
    pub description: Option<String>,
    pub summary: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
}

pub fn parse_task_notification(text: &str) -> Option<TaskNotification> {
    let body = tag_value(text, "task-notification")?;
    Some(TaskNotification {
        task_id: tag_value(&body, "task-id").unwrap_or_default(),
        status: tag_value(&body, "status").unwrap_or_default(),
        description: tag_value(&body, "description"),
        summary: tag_value(&body, "summary"),
        result: tag_value(&body, "result"),
        error: tag_value(&body, "error"),
    })
}

impl TaskNotification {
    /// Readable body: result, else error, else summary.
    pub fn body(&self) -> String {
        self.result
            .clone()
            .or_else(|| self.error.clone())
            .or_else(|| self.summary.clone())
            .unwrap_or_default()
    }
}

/// Generic tool output for display: reminders removed, TaskOutput reduced to
/// its output, a task notification reduced to its readable body.
pub fn clean_tool_output(text: &str) -> String {
    let text = strip_system_reminders(text);
    if let Some(t) = parse_task_output(&text) {
        return t.output.unwrap_or_default();
    }
    if let Some(n) = parse_task_notification(&text) {
        return n.body();
    }
    text
}

#[cfg(test)]
#[path = "tagged_text_tests.rs"]
mod tests;
