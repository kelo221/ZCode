//! Block-walk tests for `shared/markdown/mod.rs`.

use super::*;
use crate::shared::markdown::inline::render_inlines;

/// The shape of an approved plan: tight list items whose text is followed
/// directly by a nested list, in both ordered and unordered outer lists.
const PLAN: &str = "# Plan

## What you get

1. **Icons** in four places:
   - Add menu rows.
   - Outliner rows.
2. **Lights** change the radius.
   - S and the gizmo.

## Phase A

- `icon_name.ts` defines the type.
- `icons.ts` holds the data.
  - Fetched once while implementing.
  - Nothing downloads at runtime.
- Icons: bulb, beam.

## Phase B

Tail paragraph.
";

fn events(md: &str) -> Vec<Event<'_>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS);
    Parser::new_ext(md, opts).collect()
}

fn heading_text(events: &[Event], at: usize) -> String {
    let mut pos = at + 1;
    let level = match &events[at] {
        Event::Start(Tag::Heading { level, .. }) => *level,
        other => panic!("expected a heading at {at}, got {other:?}"),
    };
    render_inlines(
        events,
        &mut pos,
        Some(&TagEnd::Heading(level)),
        &Style::default(),
    )
    .text
}

/// Walk the list starting at `start` and return the index it stopped at.
fn list_end(events: &[Event], start: usize) -> usize {
    let Event::Start(Tag::List(first)) = &events[start] else {
        panic!("expected a list at {start}");
    };
    let mut pos = start + 1;
    let (mut block_ix, mut ids) = (0, 0);
    render_list(
        events,
        &mut pos,
        1,
        &mut block_ix,
        *first,
        0,
        TagEnd::List(first.is_some()),
        &mut ids,
    );
    pos
}

#[test]
fn tight_item_text_stops_before_its_nested_list() {
    let events = events("1. Icons in four places:\n   - Add menu rows.\n");
    // Past Start(List) and Start(Item).
    let mut pos = 2;
    let inline = render_inlines(&events, &mut pos, None, &Style::default());
    assert_eq!(inline.text, "Icons in four places:");
    assert!(
        matches!(events[pos], Event::Start(Tag::List(None))),
        "the nested list is left for the block walker, got {:?}",
        events[pos]
    );
}

#[test]
fn lists_with_nested_lists_end_at_their_own_end_tag() {
    let events = events(PLAN);
    let lists: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::Start(Tag::List(_))))
        .map(|(i, _)| i)
        .collect();
    // Outer ordered list, its two nested lists, outer bullet list, its nested list.
    assert_eq!(lists.len(), 5);
    let after_ordered = list_end(&events, lists[0]);
    assert_eq!(heading_text(&events, after_ordered), "Phase A");
    let after_bullets = list_end(&events, lists[3]);
    assert_eq!(heading_text(&events, after_bullets), "Phase B");
}

#[test]
fn the_whole_plan_is_walked() {
    let events = events(PLAN);
    let mut pos = 0;
    let (mut block_ix, mut ids) = (0, 0);
    let top = blocks(&events, &mut pos, 1, &mut block_ix, 0, &mut ids);
    assert_eq!(pos, events.len(), "nothing after a nested list is dropped");
    // H1, H2, ordered list, H2, bullet list, H2, paragraph.
    assert_eq!(top.len(), 7);
}

#[test]
fn a_stray_top_level_end_does_not_drop_the_rest() {
    let mut events = events("first\n\nsecond\n");
    events.insert(0, Event::End(TagEnd::Item));
    let mut pos = 0;
    let (mut block_ix, mut ids) = (0, 0);
    let top = blocks(&events, &mut pos, 1, &mut block_ix, 0, &mut ids);
    assert_eq!(pos, events.len());
    assert_eq!(top.len(), 2);
}
