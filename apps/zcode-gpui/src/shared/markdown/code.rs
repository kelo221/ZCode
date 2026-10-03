//! Code block rendering: syntect syntax highlighting mapped to gpui spans,
//! wrapped in a card with a language label and a Copy button.
//!
//! Spec: PARITY.md M2 "Code highlighting (syntect) + copy button".

use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb,
};
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

fn theme() -> &'static syntect::highlighting::Theme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    THEMES
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .get("base16-ocean.dark")
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
    (code, lang).hash(&mut hasher);
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

/// Render one highlighted line as a single flowing text element (spans as
/// runs — separate divs would stack each colored chunk vertically).
fn code_line(spans: &[CodeSpan]) -> gpui::AnyElement {
    use gpui::{
        Font, FontFeatures, FontStyle, FontWeight, StrikethroughStyle, StyledText, TextRun,
    };

    if spans.is_empty() {
        return div().child(" ").into_any_element();
    }
    let mut text = String::new();
    let mut runs = Vec::with_capacity(spans.len());
    for s in spans {
        if s.text.is_empty() {
            continue;
        }
        text.push_str(&s.text);
        runs.push(TextRun {
            len: s.text.len(),
            font: Font {
                family: CODE_FONT.into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: if s.bold {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                },
                style: if s.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
            },
            color: rgb(s.color).into(),
            background_color: None,
            underline: None,
            strikethrough: Option::<StrikethroughStyle>::None,
        });
    }
    if text.is_empty() {
        return div().child(" ").into_any_element();
    }
    StyledText::new(text).with_runs(runs).into_any_element()
}

/// Fenced code block card: header with language label + Copy, highlighted body.
pub fn code_block(id: u64, code: &str, lang: &str) -> AnyElement {
    let label = if lang.is_empty() { "text" } else { lang };
    let lines = highlight_cached(code, label);
    let copy_text = code.to_string();

    div()
        .my_1p5()
        .w_full()
        .overflow_hidden()
        .rounded_md()
        .border_1()
        .border_color(rgb(crate::shared::theme::CODE_BORDER))
        .bg(rgb(crate::shared::theme::CODE_BG))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .bg(rgb(crate::shared::theme::CODE_HEADER))
                .child(
                    div()
                        .font_family(CODE_FONT)
                        .text_size(px(10.))
                        .text_color(rgb(crate::shared::theme::MUTED))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .id(("code-copy", id))
                        .px_1p5()
                        .text_size(px(10.))
                        .text_color(rgb(crate::shared::theme::MUTED))
                        .hover(|s| s.text_color(rgb(crate::shared::theme::TEXT)))
                        .cursor_pointer()
                        .child("Copy")
                        .on_click(move |_, _, cx: &mut gpui::App| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                copy_text.clone(),
                            ));
                        }),
                ),
        )
        .child(
            div()
                .px_2()
                .py_1p5()
                .text_size(px(11.5))
                .text_color(rgb(crate::shared::theme::TEXT))
                .children(lines.iter().map(|l| code_line(l))),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
