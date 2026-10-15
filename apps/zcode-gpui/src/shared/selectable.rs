//! Read-only text the pointer can select and copy, like browser text on
//! Desktop: drag (also past the element's edge), Shift+click to extend,
//! double-click for a word, triple-click for the whole block, Ctrl/Cmd+C and
//! Ctrl/Cmd+A while focused. Clicking anywhere else clears the selection.
//!
//! Selection lives in keyed element state, so an id only has to be unique
//! inside its nearest id scope (a markdown document, tool body or bubble).

use crate::shared::theme::ACCENT;
use crate::shared::theme_colors::color as rgb;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClipboardItem, CursorStyle, DispatchPhase, ElementId, FocusHandle, HighlightStyle, Hsla,
    InteractiveElement, InteractiveText, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, RenderOnce, SharedString, Styled, StyledText, TextLayout, TextRun,
    Window, canvas, div,
};
use std::ops::Range;

struct Selection {
    focus: FocusHandle,
    anchor: usize,
    head: usize,
    dragging: bool,
}

impl Selection {
    fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

#[derive(IntoElement)]
pub(crate) struct SelectableText {
    id: ElementId,
    text: SharedString,
    runs: Option<Vec<TextRun>>,
    links: Vec<(Range<usize>, String)>,
}

impl SelectableText {
    pub(crate) fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            runs: None,
            links: Vec::new(),
        }
    }

    /// Explicit styling; the runs must cover the whole text.
    pub(crate) fn runs(mut self, runs: Vec<TextRun>) -> Self {
        self.runs = Some(runs);
        self
    }

    /// Clickable ranges opened as URLs (markdown links).
    pub(crate) fn links(mut self, links: Vec<(Range<usize>, String)>) -> Self {
        self.links = links;
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

fn index_at(layout: &TextLayout, text: &str, position: gpui::Point<gpui::Pixels>) -> usize {
    let (Ok(ix) | Err(ix)) = layout.index_for_position(position);
    snap(text, ix)
}

/// Word (letters, digits, `_`) around `ix`; any other character selects itself.
pub(crate) fn word_at(text: &str, ix: usize) -> Range<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let ix = snap(text, ix);
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

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| Selection {
            focus: cx.focus_handle(),
            anchor: 0,
            head: 0,
            dragging: false,
        });
        // Streaming text only grows; keep the selection and just clamp it.
        let (anchor, head) = {
            let s = state.read(cx);
            (s.anchor, s.head)
        };
        if snap(&text, anchor) != anchor || snap(&text, head) != head {
            state.update(cx, |s, _| {
                s.anchor = snap(&text, s.anchor);
                s.head = snap(&text, s.head);
            });
        }
        let (focus, range, dragging) = {
            let s = state.read(cx);
            (s.focus.clone(), s.range(), s.dragging)
        };

        let styled = match self.runs {
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

        let down = state.clone();
        let out = state.clone();
        let keys = state.clone();
        let down_layout = layout.clone();
        let (down_text, keys_text) = (text.clone(), text.clone());
        div()
            .id(self.id)
            .track_focus(&focus)
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                let ix = index_at(&down_layout, &down_text, ev.position);
                down.update(cx, |s, cx| {
                    window.focus(&s.focus, cx);
                    match ev.click_count {
                        1 => {
                            if !ev.modifiers.shift {
                                s.anchor = ix;
                            }
                            s.head = ix;
                            s.dragging = true;
                        }
                        2 => {
                            let word = word_at(&down_text, ix);
                            (s.anchor, s.head) = (word.start, word.end);
                        }
                        _ => (s.anchor, s.head) = (0, down_text.len()),
                    }
                    cx.notify();
                });
            })
            .on_mouse_down_out(move |_, _, cx| {
                out.update(cx, |s, cx| {
                    s.dragging = false;
                    if s.anchor != s.head {
                        s.anchor = s.head;
                        cx.notify();
                    }
                });
            })
            .on_key_down(move |ev, _, cx| {
                let stroke = &ev.keystroke;
                if !stroke.modifiers.secondary() {
                    return;
                }
                match stroke.key.as_str() {
                    "c" => {
                        let range = keys.read(cx).range();
                        if !range.is_empty() {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                keys_text[range].to_string(),
                            ));
                            cx.stop_propagation();
                        }
                    }
                    "a" => {
                        keys.update(cx, |s, cx| {
                            (s.anchor, s.head) = (0, keys_text.len());
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            })
            .child(content)
            // Window-level listeners while dragging: element mouse-move only
            // fires over the element, so a drag past its edge would stall.
            .when(dragging, |el| {
                el.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let (drag, up) = (state.clone(), state.clone());
                            window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                                if phase != DispatchPhase::Bubble {
                                    return;
                                }
                                if ev.pressed_button != Some(MouseButton::Left) {
                                    drag.update(cx, |s, _| s.dragging = false);
                                    return;
                                }
                                let ix = index_at(&layout, &text, ev.position);
                                drag.update(cx, |s, cx| {
                                    if s.head != ix {
                                        s.head = ix;
                                        cx.notify();
                                    }
                                });
                            });
                            window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left
                                {
                                    up.update(cx, |s, cx| {
                                        s.dragging = false;
                                        cx.notify();
                                    });
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}

#[cfg(test)]
#[path = "selectable_tests.rs"]
mod tests;
