//! Unified-diff rendering with the theme's diff tokens, shown for tool
//! outputs that carry patch content (edit/file-write tools).
//!
//! The +/- marker sits in its own gutter column; the code after it is one
//! selection across the whole diff, so a drag copies code without markers.
//!
//! Spec: PARITY.md M2 "Diff view (fileChanges/patch rendering)"; colors map
//! to DESIGN.md `--color-diff-added` / `--color-diff-removed`.

use crate::shared::selectable::SelectableText;
use crate::shared::selection_group::{SelectionGroup, selection_menu, selection_owner};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::menus::ContextMenu;
use gpui::{
    AnyElement, ElementId, Entity, IntoElement, ParentElement, RenderOnce, Styled, div, prelude::*,
    px,
};
use std::hash::{Hash, Hasher};

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

/// (gutter marker, selectable code) of one line. Code lines lose their
/// one-character +/-/space prefix; headers keep their whole text.
fn split_marker(line: &str, kind: LineKind) -> (&str, &str) {
    match kind {
        LineKind::Add | LineKind::Del | LineKind::Context if !line.is_empty() => {
            let cut = line.chars().next().map_or(0, char::len_utf8);
            if kind == LineKind::Context && !line.starts_with(' ') {
                ("", line)
            } else {
                line.split_at(cut)
            }
        }
        _ => ("", line),
    }
}

fn diff_line(line: &str, kind: LineKind, ord: usize, group: &Entity<SelectionGroup>) -> gpui::Div {
    let (bg, fg) = match kind {
        LineKind::Add => (Some(DIFF_ADD_BG), TEXT),
        LineKind::Del => (Some(DIFF_DEL_BG), TEXT),
        LineKind::Hunk => (Some(DIFF_HUNK_BG), MUTED),
        LineKind::Meta => (None, MUTED),
        LineKind::Context => (None, TEXT),
    };
    let (marker, code) = split_marker(line, kind);
    let mut row = div()
        .w_full()
        .flex()
        .font_family(crate::shared::theme::MONO_FONT)
        .text_size(px(11.5))
        .text_color(rgb(fg));
    if let Some(bg) = bg {
        row = row.bg(rgb(bg));
    }
    let code = if code.is_empty() { " " } else { code };
    row.child(div().flex_none().w(px(10.)).child(marker.to_string()))
        .child(
            div().flex_1().min_w_0().child(
                SelectableText::new(
                    ElementId::NamedInteger("diff-line".into(), ord as u64),
                    code.to_string(),
                )
                .group(group, ord),
            ),
        )
}

/// Render a unified diff body inside a mono card.
pub fn render_diff(text: &str) -> AnyElement {
    DiffView {
        text: text.to_string(),
    }
    .into_any_element()
}

#[derive(IntoElement)]
struct DiffView {
    text: String,
}

impl RenderOnce for DiffView {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        const MAX_LINES: usize = 2000;
        let text = self.text;
        let total = text.lines().count();
        // The diff text is the only stable identity a tool output gives us.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        let group =
            SelectionGroup::keyed(ElementId::NamedInteger("diff-sel".into(), key), window, cx);
        group.read(cx).begin_frame();
        let lines: Vec<gpui::Div> = text
            .lines()
            .take(MAX_LINES)
            .enumerate()
            .map(|(ord, line)| diff_line(line, classify(line), ord, &group))
            .collect();
        let menu = selection_menu(
            &group,
            Some((
                crate::shared::i18n::label("Copy patch", "复制补丁"),
                text.clone(),
            )),
            cx,
        );
        let root = div()
            .id(ElementId::NamedInteger("diff".into(), key))
            .my_1p5()
            .w_full()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(rgb(CODE_BORDER))
            .bg(rgb(CODE_BG))
            .px_2()
            .py_1p5()
            .child(
                ContextMenu::new(ElementId::NamedInteger("diff-ctx".into(), key), menu)
                    .child(div().w_full().children(lines)),
            )
            .when(total > MAX_LINES, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(format!("… {} more diff lines", total - MAX_LINES)),
                )
            });
        selection_owner(root, &group, cx)
    }
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
