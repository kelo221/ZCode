//! Golden tests for `shared/markdown/inline.rs` (kept out-of-line for the 400-line cap).

use super::*;
use pulldown_cmark::{Options, Parser};

fn parse_events(md: &str) -> Vec<Event<'_>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    Parser::new_ext(md, opts).collect()
}

/// Runs must always cover the flattened text exactly — gpui asserts on
/// partial run coverage at paint time.
fn assert_runs_cover(inline: &Inline) {
    let covered: usize = inline.runs.iter().map(|r| r.len).sum();
    assert_eq!(covered, inline.text.len(), "runs must cover the text");
}

#[test]
fn tight_list_bold_lead_keeps_trailing_text() {
    // Regression: the inline walker used to stop at End(Strong), dropping
    // everything after a bold lead in tight-list items ("- **Build:** rest").
    let events = parse_events("- **Build:** the rest of the line\n- plain tail");
    // Walk to the first Item's inline content (past Start(List)/Start(Item)).
    let mut pos = 2;
    let inline = render_inlines(&events, &mut pos, None, &Style::default());
    assert_eq!(inline.text, "Build: the rest of the line");
    assert_runs_cover(&inline);
    // The item end must be left for the block walker.
    assert!(matches!(events[pos], Event::End(TagEnd::Item)));
}

#[test]
fn paragraph_consumes_through_nested_ends() {
    let events = parse_events("plain **bold** and `code` tail");
    // Skip Start(Paragraph).
    let mut pos = 1;
    let inline = render_inlines(
        &events,
        &mut pos,
        Some(&TagEnd::Paragraph),
        &Style::default(),
    );
    assert_eq!(inline.text, "plain bold and code tail");
    assert_runs_cover(&inline);
    // Runs split per formatting change: plain / bold / plain / code / plain.
    assert_eq!(inline.runs.len(), 5, "{:?}", inline.runs);
    assert!(inline.runs[1].font.weight == gpui::FontWeight::BOLD);
    assert_eq!(
        inline.runs[3].font.family.as_ref(),
        crate::shared::theme::MONO_FONT
    );
}

#[test]
fn link_runs_are_clickable_and_underlined() {
    let events = parse_events("see [the docs](https://example.com/x) now");
    let mut pos = 1;
    let inline = render_inlines(
        &events,
        &mut pos,
        Some(&TagEnd::Paragraph),
        &Style::default(),
    );
    assert_eq!(inline.text, "see the docs now");
    assert_eq!(inline.links.len(), 1);
    let (range, url) = &inline.links[0];
    assert_eq!((range.start, range.end), (4, 12));
    assert_eq!(url, "https://example.com/x");
    assert_runs_cover(&inline);
    let link_run = inline.runs.iter().find(|r| r.underline.is_some()).unwrap();
    assert_eq!(link_run.color, c(crate::shared::theme::LINK));
}

#[test]
fn inline_code_gets_mono_font_and_background() {
    let events = parse_events("run `bun run gpui` now");
    let mut pos = 1;
    let inline = render_inlines(
        &events,
        &mut pos,
        Some(&TagEnd::Paragraph),
        &Style::default(),
    );
    assert_eq!(inline.text, "run bun run gpui now");
    let code_run = inline
        .runs
        .iter()
        .find(|r| r.background_color.is_some())
        .unwrap();
    assert_eq!(
        code_run.font.family.as_ref(),
        crate::shared::theme::MONO_FONT
    );
    assert_runs_cover(&inline);
}

#[test]
fn strikethrough_does_not_merge_into_adjacent_code() {
    // Struck and plain code share font/color/background; only the
    // strikethrough differs, so they must stay separate runs.
    let events = parse_events("~~`a`~~`b`");
    let mut pos = 1;
    let inline = render_inlines(
        &events,
        &mut pos,
        Some(&TagEnd::Paragraph),
        &Style::default(),
    );
    assert_eq!(inline.text, "ab");
    assert_runs_cover(&inline);
    assert_eq!(inline.runs.len(), 2);
    assert!(inline.runs[0].strikethrough.is_some());
    assert!(inline.runs[1].strikethrough.is_none());
}
