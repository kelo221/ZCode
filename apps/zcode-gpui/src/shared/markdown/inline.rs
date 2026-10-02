//! Inline markdown → gpui `StyledText` runs. Inline content must render as
//! ONE flowing text element (like the desktop app); a div per span renders
//! block-level and breaks every span onto its own line. Links stay clickable
//! via `InteractiveText` range callbacks.
//!
//! Spec: PARITY.md M2 "Streaming Markdown renderer".

use gpui::{
    px, rgb, AnyElement, Font, FontFeatures, FontStyle, FontWeight, Hsla,
    InteractiveText, IntoElement, StrikethroughStyle, StyledText, TextRun, UnderlineStyle,
};
use pulldown_cmark::{Event, Tag, TagEnd};
use std::ops::Range;

const BODY_FONT: &str = "Segoe UI";
const CODE_FONT: &str = "Consolas";

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Flattened inline content: one string + one style run per formatting span
/// (runs concatenate to exactly `text`) + clickable link ranges.
#[derive(Default)]
pub(crate) struct Inline {
    pub text: String,
    pub runs: Vec<TextRun>,
    /// (byte range into `text`, url) — order matches click listener indices.
    pub links: Vec<(Range<usize>, String)>,
}

/// Character formatting accumulated across nested containers.
#[derive(Clone)]
pub(crate) struct Style {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

impl Style {
    pub(crate) fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            strike: false,
            code: false,
            link: false,
        }
    }
}

fn font_of(st: &Style) -> Font {
    Font {
        family: if st.code { CODE_FONT.into() } else { BODY_FONT.into() },
        features: FontFeatures::default(),
        fallbacks: None,
        weight: if st.bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        style: if st.italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

fn color_of(st: &Style) -> Hsla {
    if st.link {
        c(crate::shared::theme::LINK)
    } else if st.code {
        c(0xe6b67d)
    } else if st.strike {
        c(crate::shared::theme::MUTED)
    } else {
        c(crate::shared::theme::TEXT)
    }
}

/// Append `len` bytes styled by `st`, merging with the previous run when the
/// formatting is identical (fewer runs = faster shaping).
fn push_run(inline: &mut Inline, len: usize, st: &Style) {
    if len == 0 {
        return;
    }
    let font = font_of(st);
    let color = color_of(st);
    let background = st.code.then_some(c(crate::shared::theme::CODE_BG));
    let underline = st.link.then(|| UnderlineStyle {
        thickness: px(1.),
        color: None,
        wavy: false,
    });
    let strike = st.strike.then(|| StrikethroughStyle {
        thickness: px(1.),
        color: None,
    });
    // Decorations must match too, or a strikethrough/underline would spread
    // onto (or vanish from) an adjacent span with the same font and color.
    if let Some(last) = inline.runs.last_mut()
        && last.font == font
        && last.color == color
        && last.background_color == background
        && last.underline == underline
        && last.strikethrough == strike
    {
        last.len += len;
        return;
    }
    inline.runs.push(TextRun {
        len,
        font,
        color,
        background_color: background,
        underline,
        strikethrough: strike,
    });
}

/// Consume inline events from `events[*pos..]` into one `Inline`. Stops at
/// (and consumes) the matching `stop` end tag, or — with `stop = None` — at
/// the first block-level `End` (tight-list content), which it leaves for the
/// block walker. Inline-level ends (e.g. `End(Strong)`) are always consumed:
/// stopping on those would truncate the item after its bold lead.
pub(crate) fn render_inlines(
    events: &[Event],
    pos: &mut usize,
    stop: Option<&TagEnd>,
    base: &Style,
) -> Inline {
    let mut inline = Inline::default();
    let st = base.clone();
    while *pos < events.len() {
        match &events[*pos] {
            Event::End(end) => match stop {
                Some(stop_tag) if end == stop_tag => {
                    *pos += 1;
                    return inline;
                }
                None if is_block_end(end) => return inline, // left for the block walker
                _ => *pos += 1,
            },
            Event::Text(t) => {
                push_run(&mut inline, t.len(), &st);
                inline.text.push_str(t);
                *pos += 1;
            }
            Event::Code(cd) => {
                let mut code_st = st.clone();
                code_st.code = true;
                push_run(&mut inline, cd.len(), &code_st);
                inline.text.push_str(cd);
                *pos += 1;
            }
            Event::SoftBreak | Event::HardBreak => {
                push_run(&mut inline, 1, &st);
                inline.text.push(' ');
                *pos += 1;
            }
            Event::Start(tag @ (Tag::Strong | Tag::Emphasis | Tag::Strikethrough | Tag::Link { .. })) => {
                let mut inner = st.clone();
                let mut url = None;
                match tag {
                    Tag::Strong => inner.bold = true,
                    Tag::Emphasis => inner.italic = true,
                    Tag::Strikethrough => inner.strike = true,
                    Tag::Link { dest_url, .. } => {
                        url = Some(dest_url.to_string());
                        inner.link = true;
                    }
                    _ => {}
                }
                let end = tag.to_end();
                let start_byte = inline.text.len();
                *pos += 1;
                let inner_inline = render_inlines(events, pos, Some(&end), &inner);
                inline.text.push_str(&inner_inline.text);
                inline.runs.extend(inner_inline.runs);
                // Inner links (e.g. images inside the link text) are relative
                // to the inner text and shift by the splice offset; the link's
                // own range is already absolute.
                for (range, link) in inner_inline.links {
                    inline.links.push((range.start + start_byte..range.end + start_byte, link));
                }
                if let Some(url) = url {
                    inline.links.push((start_byte..inline.text.len(), url));
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                // v1: images render as a link to their target.
                let mut img = st.clone();
                img.link = true;
                let label = format!("[image: {dest_url}]");
                push_run(&mut inline, label.len(), &img);
                inline.text.push_str(&label);
                inline.links.push((inline.text.len() - label.len()..inline.text.len(), dest_url.to_string()));
                *pos += 1;
            }
            Event::InlineHtml(h) | Event::Html(h) => {
                push_run(&mut inline, h.len(), &st);
                inline.text.push_str(h);
                *pos += 1;
            }
            Event::TaskListMarker(done) => {
                let marker = if *done { "☑ " } else { "☐ " };
                push_run(&mut inline, marker.len(), &st);
                inline.text.push_str(marker);
                *pos += 1;
            }
            Event::FootnoteReference(f) => {
                let label = format!("[{f}]");
                push_run(&mut inline, label.len(), &st);
                inline.text.push_str(&label);
                *pos += 1;
            }
            _ => *pos += 1,
        }
    }
    inline
}

/// Block-level end tags that terminate bare (tight-list) inline content.
fn is_block_end(end: &TagEnd) -> bool {
    matches!(
        end,
        TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::Item
            | TagEnd::List(_)
            | TagEnd::BlockQuote(_)
            | TagEnd::Table
            | TagEnd::TableHead
            | TagEnd::TableRow
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::FootnoteDefinition
    )
}

/// Render the flattened inline content; links become clickable ranges.
pub(crate) fn inline_element(inline: Inline, id: gpui::ElementId) -> AnyElement {
    if inline.text.is_empty() {
        return gpui::div().into_any_element();
    }
    let styled = StyledText::new(inline.text).with_runs(inline.runs);
    if inline.links.is_empty() {
        return styled.into_any_element();
    }
    let urls: Vec<String> = inline.links.iter().map(|(_, u)| u.clone()).collect();
    let ranges: Vec<Range<usize>> = inline.links.into_iter().map(|(r, _)| r).collect();
    InteractiveText::new(id, styled)
        .on_click(ranges, move |ix, _window, cx| {
            if let Some(url) = urls.get(ix) {
                cx.open_url(url);
            }
        })
        .into_any_element()
}

#[cfg(test)]
#[path = "inline_tests.rs"]
mod tests;
