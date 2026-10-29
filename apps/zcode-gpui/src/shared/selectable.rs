//! Read-only text the pointer can select and copy, like browser text on
//! Desktop: drag (also past the element's edge), Shift+click to extend,
//! double-click for a word, triple-click for the whole block (dragging after
//! either extends by words / blocks), Ctrl/Cmd+C and Ctrl/Cmd+A while
//! focused. Clicking anywhere else clears the selection.
//!
//! A [`SelectionGroup`](crate::shared::selection_group::SelectionGroup) owns one selection over several text blocks (a whole
//! markdown document), so a drag runs from one paragraph or list item into the
//! next. A `SelectableText` without a group owns a private one-block group.
//!
//! Selection lives in keyed element state, so an id only has to be unique
//! inside its nearest id scope (a markdown document, tool body or bubble).

use crate::shared::selection_group::{SelectionGroup, Slot, selection_menu, selection_owner};
use crate::shared::theme::ACCENT;
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::menus::ContextMenu;
use gpui::{
    App, CursorStyle, ElementId, Entity, HighlightStyle, Hsla, InteractiveElement, InteractiveText,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, RenderOnce,
    SharedString, Styled, StyledText, TextLayout, TextRun, Window, canvas, div,
};
use std::ops::Range;
#[derive(IntoElement)]
pub(crate) struct SelectableText {
    id: ElementId,
    text: SharedString,
    runs: Option<Vec<TextRun>>,
    /// Highlights over the inherited text style (ignored when `runs` is set).
    styles: Vec<(Range<usize>, HighlightStyle)>,
    links: Vec<(Range<usize>, String)>,
    group: Option<(Entity<SelectionGroup>, usize)>,
}

impl SelectableText {
    pub(crate) fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            runs: None,
            styles: Vec::new(),
            links: Vec::new(),
            group: None,
        }
    }

    /// Explicit styling; the runs must cover the whole text.
    pub(crate) fn runs(mut self, runs: Vec<TextRun>) -> Self {
        self.runs = Some(runs);
        self
    }

    /// Sorted, disjoint ranges styled on top of the inherited text style.
    pub(crate) fn styles(mut self, styles: Vec<(Range<usize>, HighlightStyle)>) -> Self {
        self.styles = styles;
        self
    }

    /// Clickable ranges opened as URLs (markdown links).
    pub(crate) fn links(mut self, links: Vec<(Range<usize>, String)>) -> Self {
        self.links = links;
        self
    }

    /// Join a shared selection as block `ord` (document order). The group's
    /// owner element must be wrapped with [`selection_owner`].
    pub(crate) fn group(mut self, group: &Entity<SelectionGroup>, ord: usize) -> Self {
        self.group = Some((group.clone(), ord));
        self
    }
}

fn selection_color() -> Hsla {
    let mut bg = rgb(ACCENT);
    bg.a = 0.35;
    bg.into()
}

pub(crate) fn snap(text: &str, ix: usize) -> usize {
    let mut ix = ix.min(text.len());
    while !text.is_char_boundary(ix) {
        ix -= 1;
    }
    ix
}

pub(crate) fn index_at(layout: &TextLayout, text: &str, position: Point<Pixels>) -> usize {
    let (Ok(ix) | Err(ix)) = layout.index_for_position(position);
    snap(text, ix)
}

/// Word (letters, digits, `_`) around `ix`; any other character selects itself.
pub(crate) fn word_at(text: &str, ix: usize) -> Range<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut ix = snap(text, ix);
    // A click on the right half of a word's last character lands on the
    // boundary after it; that still means the word, not the next character.
    if !text[ix..].chars().next().is_some_and(is_word)
        && let Some(prev) = text[..ix].chars().next_back().filter(|&c| is_word(c))
    {
        ix -= prev.len_utf8();
    }
    let Some(c) = text[ix..].chars().next() else {
        return ix..ix;
    };
    if !is_word(c) {
        return ix..ix + c.len_utf8();
    }
    let start = text[..ix]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word(c))
        .last()
        .map_or(ix, |(i, _)| i);
    let end = text[ix..]
        .char_indices()
        .find(|&(_, c)| !is_word(c))
        .map_or(text.len(), |(i, _)| ix + i);
    start..end
}

/// Split `runs` at the selection edges and give the selected part a background.
pub(crate) fn highlight_runs(runs: Vec<TextRun>, sel: Range<usize>, bg: Hsla) -> Vec<TextRun> {
    if sel.is_empty() {
        return runs;
    }
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let mut cuts = vec![start];
        for edge in [sel.start, sel.end] {
            if edge > start && edge < end {
                cuts.push(edge);
            }
        }
        cuts.push(end);
        for pair in cuts.windows(2) {
            let mut piece = run.clone();
            piece.len = pair[1] - pair[0];
            if pair[0] >= sel.start && pair[1] <= sel.end {
                piece.background_color = Some(bg);
            }
            out.push(piece);
        }
        start = end;
    }
    out
}

/// Runs covering `text`: the base style, with `styles` applied over their ranges.
pub(crate) fn styled_runs(
    base: &gpui::TextStyle,
    text: &str,
    styles: &[(Range<usize>, HighlightStyle)],
) -> Vec<TextRun> {
    let mut runs = Vec::with_capacity(styles.len() * 2 + 1);
    let mut at = 0;
    for (range, style) in styles {
        let range = range.start.clamp(at, text.len())..range.end.clamp(at, text.len());
        if range.start > at {
            runs.push(base.to_run(range.start - at));
        }
        if !range.is_empty() {
            runs.push(base.clone().highlight(*style).to_run(range.len()));
        }
        at = range.end;
    }
    if at < text.len() {
        runs.push(base.to_run(text.len() - at));
    }
    runs
}

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text;
        let (group, ord, owned) = match self.group {
            Some((group, ord)) => (group, ord, false),
            None => (SelectionGroup::keyed(self.id.clone(), window, cx), 0, true),
        };
        let (range, slots) = {
            let g = group.read(cx);
            (g.block_range(ord, &text), g.slots.clone())
        };

        let runs = self.runs.or_else(|| {
            (!self.styles.is_empty())
                .then(|| styled_runs(&window.text_style(), &text, &self.styles))
        });
        let styled = match runs {
            Some(runs) => StyledText::new(text.clone()).with_runs(highlight_runs(
                runs,
                range.clone(),
                selection_color(),
            )),
            None => StyledText::new(text.clone()).with_highlights((!range.is_empty()).then(|| {
                (
                    range.clone(),
                    HighlightStyle {
                        background_color: Some(selection_color()),
                        ..Default::default()
                    },
                )
            })),
        };
        let layout = styled.layout().clone();
        let content = if self.links.is_empty() {
            styled.into_any_element()
        } else {
            let (ranges, urls): (Vec<_>, Vec<_>) = self.links.into_iter().unzip();
            InteractiveText::new(self.id.clone(), styled)
                .on_click(ranges, move |ix, _, cx| {
                    if let Some(url) = urls.get(ix) {
                        cx.open_url(url);
                    }
                })
                .into_any_element()
        };

        // A document owner carries the menu for all its blocks.
        let content = if owned {
            let menu = selection_menu(&group, None, cx);
            ContextMenu::new("sel-ctx", menu)
                .child(content)
                .into_any_element()
        } else {
            content
        };
        let down = group.clone();
        let (down_layout, down_text) = (layout.clone(), text.clone());
        let block = div()
            .id(self.id)
            .relative()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                let ix = index_at(&down_layout, &down_text, ev.position);
                down.update(cx, |g, cx| {
                    window.focus(&g.focus, cx);
                    g.press((ord, ix), &down_text, ev.click_count, ev.modifiers.shift);
                    cx.notify();
                });
            })
            .child(content)
            // Registered at prepaint, once the text layout has bounds.
            .child(
                canvas(
                    move |bounds, _, _| {
                        let mut slots = slots.borrow_mut();
                        if slots.len() <= ord {
                            slots.resize_with(ord + 1, || None);
                        }
                        slots[ord] = Some(Slot {
                            text,
                            layout,
                            bounds,
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        if owned {
            selection_owner(block, &group, cx)
        } else {
            block
        }
    }
}

#[cfg(test)]
#[path = "selectable_tests.rs"]
mod tests;
