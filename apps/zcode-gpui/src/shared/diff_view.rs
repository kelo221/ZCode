//! Unified-diff rendering with the theme's diff tokens, shown for tool
//! outputs that carry patch content (edit/file-write tools).
//!
//! Spec: PARITY.md M2 "Diff view (fileChanges/patch rendering)"; colors map
//! to DESIGN.md `--color-diff-added` / `--color-diff-removed`.

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb};

use crate::shared::theme::{
    CODE_BG, CODE_BORDER, DIFF_ADD_BG, DIFF_DEL_BG, DIFF_HUNK_BG, MUTED, TEXT,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum LineKind {
    Add,
    Del,
    Hunk,
    Meta,
    Context,
}

fn classify(line: &str) -> LineKind {
    if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff ") {
        return LineKind::Meta;
    }
    if line.starts_with("@@") {
        return LineKind::Hunk;
    }
    // Trailing "\ No newline at end of file" markers stay context-colored.
    if line.starts_with('+') {
        LineKind::Add
    } else if line.starts_with('-') {
        LineKind::Del
    } else {
        LineKind::Context
    }
}

/// A tool output is treated as a patch when it carries hunk headers or a
/// coherent +/- pair — this keeps prose that merely starts with "+" plain.
pub fn looks_like_diff(text: &str) -> bool {
    let mut adds = 0usize;
    let mut dels = 0usize;
    let mut prev_minus_header = false;
    for line in text.lines().take(400) {
        // Only real patch headers are decisive: a lone "---" is just as
        // likely YAML front matter, a Markdown rule or a table border.
        if line.starts_with("@@ -") || line.starts_with("diff --git ") {
            return true;
        }
        if prev_minus_header && line.starts_with("+++ ") {
            return true;
        }
        prev_minus_header = line.starts_with("--- ");
        match classify(line) {
            LineKind::Add => adds += 1,
            LineKind::Del => dels += 1,
            LineKind::Hunk | LineKind::Meta | LineKind::Context => {}
        }
    }
    adds >= 2 && dels >= 2
}

fn diff_line(line: &str, kind: LineKind) -> gpui::Div {
    let (bg, fg) = match kind {
        LineKind::Add => (Some(DIFF_ADD_BG), TEXT),
        LineKind::Del => (Some(DIFF_DEL_BG), TEXT),
        LineKind::Hunk => (Some(DIFF_HUNK_BG), MUTED),
        LineKind::Meta => (None, MUTED),
        LineKind::Context => (None, TEXT),
    };
    let mut row = div()
        .w_full()
        .font_family("Consolas")
        .text_size(px(11.5))
        .text_color(rgb(fg));
    if let Some(bg) = bg {
        row = row.bg(rgb(bg));
    }
    row.child(if line.is_empty() { " " } else { line }.to_string())
}

/// Render a unified diff body inside a mono card.
pub fn render_diff(text: &str) -> AnyElement {
    const MAX_LINES: usize = 2000;
    let truncated = text.lines().count() > MAX_LINES;

    div()
        .my_1p5()
        .w_full()
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(rgb(CODE_BORDER))
        .bg(rgb(CODE_BG))
        .px_2()
        .py_1p5()
        .children(
            text.lines()
                .take(MAX_LINES)
                .map(|line| diff_line(line, classify(line))),
        )
        .when(truncated, |el| {
            el.child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "… {} more diff lines",
                        text.lines().count() - MAX_LINES
                    )),
            )
        })
        .into_any_element()
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
