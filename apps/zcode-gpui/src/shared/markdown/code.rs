//! Code block rendering: syntect syntax highlighting mapped to gpui spans,
//! wrapped in a card with a language label and a Copy button.
//!
//! Spec: PARITY.md M2 "Code highlighting (syntect) + copy button".

use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, px};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// One highlighted text run inside a code line.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeSpan {
    pub color: u32,
    pub bold: bool,
    pub italic: bool,
    pub text: String,
}

/// A line of highlighted code (spans concatenate to the line text).
pub type CodeLine = Vec<CodeSpan>;

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Syntax colors follow the effective light/dark mode so tokens stay legible on the
/// code block's background.
fn theme_name() -> &'static str {
    if crate::shared::theme::effective_light() {
        "InspiredGitHub"
    } else {
        "base16-ocean.dark"
    }
}

fn theme() -> &'static syntect::highlighting::Theme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .get(theme_name())
        .unwrap_or_else(|| {
            static FALLBACK: OnceLock<syntect::highlighting::Theme> = OnceLock::new();
            FALLBACK.get_or_init(syntect::highlighting::Theme::default)
        })
}

/// Entries kept by the highlight memo; cleared wholesale when full (streaming
/// blocks churn through many short-lived prefixes).
const HIGHLIGHT_CACHE_CAP: usize = 128;
/// (hash of code + language, code length).
type HighlightKey = (u64, usize);

/// Memoized `highlight`: rows re-render every frame while scrolling, and
/// syntect is far too slow to rerun per frame on large blocks.
fn highlight_cached(code: &str, lang: &str) -> std::rc::Rc<Vec<CodeLine>> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::rc::Rc;
    thread_local! {
        static CACHE: RefCell<HashMap<HighlightKey, Rc<Vec<CodeLine>>>> =
            RefCell::new(HashMap::new());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (code, lang, theme_name()).hash(&mut hasher);
    // Length in the key makes a 64-bit hash collision even less plausible.
    let key = (hasher.finish(), code.len());
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let lines = Rc::new(highlight(code, lang));
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= HIGHLIGHT_CACHE_CAP {
            c.clear();
        }
        c.insert(key, lines.clone());
    });
    lines
}

/// Highlight `code` line-by-line for `lang` (plain, single-color lines when
/// the language is unknown or the block exceeds the size cap).
pub fn highlight(code: &str, lang: &str) -> Vec<CodeLine> {
    const MAX_HIGHLIGHT_CHARS: usize = 120_000;
    let plain = || {
        code.lines()
            .map(|l| {
                vec![CodeSpan {
                    color: crate::shared::theme::TEXT,
                    bold: false,
                    italic: false,
                    text: l.to_string(),
                }]
            })
            .collect::<Vec<_>>()
    };
    if code.len() > MAX_HIGHLIGHT_CHARS {
        return plain();
    }
    // Unknown languages render in the UI text color instead of the syntect
    // theme's default, so they match the surrounding card.
    let Some(syntax) = syntax_set().find_syntax_by_token(lang) else {
        return plain();
    };
    let mut hl = HighlightLines::new(syntax, theme());
    let mut out = Vec::new();
    for line in syntect::util::LinesWithEndings::from(code) {
        let mut spans = Vec::new();
        for (style, chunk) in hl.highlight_line(line, syntax_set()).unwrap_or_default() {
            let fg = style.foreground;
            spans.push(CodeSpan {
                color: ((fg.r as u32) << 16) | ((fg.g as u32) << 8) | fg.b as u32,
                bold: style.font_style.contains(FontStyle::BOLD),
                italic: style.font_style.contains(FontStyle::ITALIC),
                text: chunk.trim_end_matches(['\r', '\n']).to_string(),
            });
        }
        out.push(spans);
    }
    if out.is_empty() {
        // Keep one blank line so empty fences still have height.
        return vec![vec![CodeSpan {
            color: crate::shared::theme::TEXT,
            bold: false,
            italic: false,
            text: String::new(),
        }]];
    }
    out
}

const CODE_FONT: &str = crate::shared::theme::MONO_FONT;

/// The whole block as one text with newline-joined lines and spans as runs,
/// so a selection can cover several lines (separate line elements could not).
pub(crate) fn code_text(lines: &[CodeLine]) -> (String, Vec<gpui::TextRun>) {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TextRun};

    let run = |len: usize, color: u32, bold: bool, italic: bool| TextRun {
        len,
        font: Font {
            family: CODE_FONT.into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: if bold {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            style: if italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
        },
        color: crate::shared::theme_colors::color(color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut text = String::new();
    let mut runs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            text.push('\n');
            runs.push(run(1, crate::shared::theme::TEXT, false, false));
        }
        for s in line.iter().filter(|s| !s.text.is_empty()) {
            text.push_str(&s.text);
            runs.push(run(s.text.len(), s.color, s.bold, s.italic));
        }
    }
    if text.is_empty() {
        // An empty block still shows one blank line.
        text.push(' ');
        runs.push(run(1, crate::shared::theme::TEXT, false, false));
    }
    (text, runs)
}

/// Fenced code block card: header with language label + Copy, highlighted body.
pub fn code_block(id: u64, code: &str, lang: &str) -> AnyElement {
    let label = if lang.is_empty() { "text" } else { lang };
    let lines = highlight_cached(code, label);
    let copy_text = code.to_string();

    let color = crate::shared::theme_colors::color;
    use crate::shared::theme::{BORDER, CODE_BG, MUTED, TEXT, ui_size};
    div()
        .my_1p5()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(color(BORDER))
        .bg(color(CODE_BG))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
                .child(
                    div()
                        .font_family(CODE_FONT)
                        .text_size(px(ui_size(12.)))
                        .text_color(color(MUTED))
                        .child(label.to_string()),
                )
                .child(
                    IconButton::new(("code-copy", id), IconName::Copy)
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .tooltip("Copy")
                        .on_click(move |_, _, cx: &mut gpui::App| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                copy_text.clone(),
                            ));
                        }),
                ),
        )
        .child(
            div()
                .px_3()
                .pb_3()
                .text_size(px(ui_size(12.)))
                .text_color(color(TEXT))
                .child({
                    let (text, runs) = code_text(&lines);
                    crate::shared::selectable::SelectableText::new(
                        gpui::ElementId::NamedInteger("code-text".into(), id),
                        text,
                    )
                    .runs(runs)
                }),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
