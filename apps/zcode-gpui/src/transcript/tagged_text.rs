//! Model-facing XML-ish wrappers the CLI puts into tool results and
//! background-result messages, turned into user-facing text.
//!
//! Formats: TaskOutput (`core/src/tool/handlers/task-output.ts`:
//! `<retrieval_status>`, `<task_id>`, `<status>`, `<output>`), task
//! notifications (`core/src/runtime-task/notification.ts`) and
//! `<system-reminder>` blocks, which Desktop never shows
//! (`conversation-message-projection-policy.ts`).

use std::borrow::Cow;

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

/// A subagent closes its final answer with a `<remaining_items>` block that
/// the CLI strips before the parent sees it
/// (`core/src/subagent/completion-notification.ts`), but the child's own
/// transcript keeps the raw text. Show that block as a Markdown list. Only a
/// tag at the start of a line outside code fences counts, so prose or code
/// that mentions the tag is left alone. An unclosed block runs to the end.
pub fn remaining_items_as_markdown(text: &str) -> Cow<'_, str> {
    const OPEN: &str = "<remaining_items>";
    const CLOSE: &str = "</remaining_items>";
    if !text.contains(OPEN) {
        return Cow::Borrowed(text);
    }
    let mut in_fence = false;
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        } else if !in_fence && trimmed.starts_with(OPEN) {
            start = Some(offset + (line.len() - trimmed.len()));
            break;
        }
        offset += line.len();
    }
    let Some(start) = start else {
        return Cow::Borrowed(text);
    };
    let body_start = start + OPEN.len();
    let (body, after) = match text[body_start..].find(CLOSE) {
        Some(end) => (
            &text[body_start..body_start + end],
            &text[body_start + end + CLOSE.len()..],
        ),
        None => (&text[body_start..], ""),
    };
    let items: Vec<&str> = body
        .lines()
        .map(strip_list_marker)
        .filter(|item| !item.is_empty())
        .collect();
    let mut out = text[..start].trim_end().to_string();
    if !items.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("**Unfinished items**\n\n");
        for item in items {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
    }
    let after = after.trim();
    if !after.is_empty() {
        // 空行隔开，否则 Markdown 会把后文接进列表最后一项。
        let trimmed = out.trim_end().len();
        out.truncate(trimmed);
        out.push_str("\n\n");
        out.push_str(after);
    }
    Cow::Owned(out.trim_end().to_string())
}

fn strip_list_marker(line: &str) -> &str {
    let line = line.trim();
    for marker in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest.trim();
        }
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return rest.trim();
        }
    }
    line
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
