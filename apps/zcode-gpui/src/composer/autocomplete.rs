//! Autocomplete popup for slash commands (/compact, /goal) and mentions (@file, @session, @skill, @plugin).

use crate::composer::slash::{SlashCommand, filter_slash_commands};
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD, CARD_HOVER, MUTED, TEXT, TOOL};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div, prelude::*, px,
};
use std::path::Path;

/// An autocomplete suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutocompleteSuggestion {
    Slash(SlashCommand),
    Mention {
        category: &'static str,
        label: String,
        description: String,
        insert_text: String,
    },
}

impl AutocompleteSuggestion {
    pub fn label(&self) -> &str {
        match self {
            Self::Slash(s) => &s.name,
            Self::Mention { label, .. } => label,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Slash(s) => &s.description,
            Self::Mention { description, .. } => description,
        }
    }
}

/// Recursively search workspace files matching query, skipping heavy/generated directories.
pub fn match_workspace_files(
    root: &Path,
    query: &str,
    max_results: usize,
) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut scanned = 0;

    while let Some((dir, depth)) = stack.pop() {
        if depth > 4 || scanned > 800 {
            break;
        }
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            scanned += 1;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.')
                || file_name == "node_modules"
                || file_name == "target"
                || file_name == "dist"
                || file_name == "out"
                || file_name == "bin"
                || file_name == "obj"
            {
                continue;
            }
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let path = entry.path();
            if let Ok(rel) = path.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let matches_query = query.is_empty()
                    || file_name.to_lowercase().contains(query)
                    || rel_str.to_lowercase().contains(query);
                if matches_query && !is_dir {
                    results.push((rel_str, file_name));
                    if results.len() >= max_results {
                        return results;
                    }
                }
            }
            if is_dir {
                stack.push((path, depth + 1));
            }
        }
    }
    results
}

/// Detect whether the current text should trigger slash or mention autocomplete.
pub fn detect_autocomplete(
    text: &str,
    available_slash: &[SlashCommand],
    sessions: &[(String, String)],
    workspace_root: Option<&Path>,
) -> Option<Vec<AutocompleteSuggestion>> {
    let trimmed = text.trim_start();
    if let Some(matches) = filter_slash_commands(available_slash, trimmed).filter(|m| !m.is_empty())
    {
        return Some(
            matches
                .into_iter()
                .cloned()
                .map(AutocompleteSuggestion::Slash)
                .collect(),
        );
    }

    if let Some(at_idx) = text.rfind('@') {
        let before_at = &text[..at_idx];
        if before_at.is_empty() || before_at.ends_with(' ') || before_at.ends_with('\n') {
            let query = &text[at_idx + 1..];
            if !query.contains(' ') && !query.contains('\n') {
                let q_lower = query.to_lowercase();
                let mut list = Vec::new();

                // 1. Files in workspace (max 5)
                if let Some(root) = workspace_root {
                    let files = match_workspace_files(root, &q_lower, 5);
                    for (rel_path, file_name) in files {
                        let insert = format!("[{file_name}](./{rel_path})");
                        list.push(AutocompleteSuggestion::Mention {
                            category: "file",
                            label: file_name,
                            description: rel_path,
                            insert_text: insert,
                        });
                    }
                }

                // 2. Active sessions
                for (sid, title) in sessions {
                    if q_lower.is_empty()
                        || sid.to_lowercase().contains(&q_lower)
                        || title.to_lowercase().contains(&q_lower)
                    {
                        list.push(AutocompleteSuggestion::Mention {
                            category: "session",
                            label: title.clone(),
                            description: sid.clone(),
                            insert_text: format!("[#{title}](#{sid})"),
                        });
                    }
                }

                if !list.is_empty() {
                    list.truncate(8);
                    return Some(list);
                }
            }
        }
    }

    None
}

impl crate::app::root::RootView {
    /// Render the floating autocomplete suggestion popup directly above the composer.
    pub(crate) fn render_autocomplete_popup(
        &self,
        suggestions: &[AutocompleteSuggestion],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("autocomplete-popup")
            .w_full()
            .flex()
            .flex_col()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg()
            .shadow_lg()
            .py_1()
            .mb_1()
            .children(suggestions.iter().enumerate().map(|(idx, item)| {
                let item_clone = item.clone();
                let slash_receipt = self.state.read(cx).slash_catalog_receipt(cx);
                let (prefix, icon, lead_color) = match item {
                    AutocompleteSuggestion::Slash(_) => ("/", "⚡", ACCENT),
                    AutocompleteSuggestion::Mention { category, .. } => match *category {
                        "file" => ("", "📄", TOOL),
                        "session" => ("", "#", MUTED),
                        "skill" => ("", "⚡", ACCENT),
                        "plugin" => ("", "🔌", TEXT),
                        _ => ("", "@", MUTED),
                    },
                };

                let row = div()
                    .id(SharedString::from(format!("auto-item-{idx}")))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(CARD_HOVER)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(ui_size(11.)))
                                    .text_color(rgb(lead_color))
                                    .child(icon),
                            )
                            .child(
                                div()
                                    .text_size(px(ui_size(12.5)))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(lead_color))
                                    .child(format!("{prefix}{}", item.label())),
                            )
                            .child(
                                div()
                                    .text_size(px(ui_size(11.)))
                                    .text_color(rgb(MUTED))
                                    .child(item.description().to_string()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let AutocompleteSuggestion::Slash(command) = &item_clone {
                            if let Some(receipt) = &slash_receipt {
                                this.state.update(cx, |state, cx| {
                                    state.apply_slash_suggestion(receipt, command, cx);
                                });
                            }
                        } else {
                            this.apply_autocomplete(&item_clone, cx);
                        }
                    }));
                let wrapper = div().child(row);
                #[cfg(test)]
                let wrapper = crate::app::test_support::track_children(
                    wrapper,
                    vec![format!("auto-item-{idx}")],
                );
                wrapper
            }))
            .into_any_element()
    }

    /// Apply the chosen suggestion into the composer input.
    pub(crate) fn apply_autocomplete(
        &self,
        suggestion: &AutocompleteSuggestion,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, cx| {
            if let AutocompleteSuggestion::Slash(command) = suggestion {
                if let Some(receipt) = s.slash_catalog_receipt(cx) {
                    s.apply_slash_suggestion(&receipt, command, cx);
                }
                return;
            }
            s.composer.update(cx, |c, _ccx| {
                let text = c.text().to_string();
                match suggestion {
                    AutocompleteSuggestion::Slash(_) => {}
                    // Mentions become atomic chips (side-table ranges), so
                    // backspace / IME never leave half a link behind.
                    AutocompleteSuggestion::Mention { insert_text, .. } => {
                        let start = crate::composer::references::reference_query(&text)
                            .map(|(_, _, idx)| idx)
                            .unwrap_or(text.len());
                        c.insert_chip(start..text.len(), insert_text);
                    }
                }
            });
        });
        cx.notify();
    }
}

#[cfg(test)]
#[path = "autocomplete_tests.rs"]
mod tests;
