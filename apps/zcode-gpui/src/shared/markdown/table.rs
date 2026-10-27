//! Markdown tables: one selectable block per cell, in row-major order.

use crate::shared::markdown::inline::{Style, inline_element, render_inlines};
use crate::shared::selection_group::SelectionGroup;
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Entity, IntoElement, ParentElement, Styled, div, prelude::*};
use pulldown_cmark::{Event, Tag, TagEnd};

pub(super) fn render_table(
    events: &[Event],
    pos: &mut usize,
    aligns: &[pulldown_cmark::Alignment],
    block_ix: &mut usize,
    group: &Entity<SelectionGroup>,
) -> AnyElement {
    let mut rows: Vec<(bool, Vec<Vec<AnyElement>>)> = Vec::new();
    while *pos < events.len() {
        match &events[*pos] {
            Event::End(TagEnd::Table) => {
                *pos += 1;
                break;
            }
            Event::Start(tag @ (Tag::TableHead | Tag::TableRow)) => {
                let head = *tag == Tag::TableHead;
                *pos += 1;
                let mut row: Vec<Vec<AnyElement>> = Vec::new();
                while *pos < events.len() {
                    match &events[*pos] {
                        Event::End(TagEnd::TableCell) => {
                            *pos += 1;
                        }
                        Event::Start(Tag::TableCell) => {
                            *pos += 1;
                            let inline = render_inlines(
                                events,
                                pos,
                                Some(&TagEnd::TableCell),
                                &Style::default(),
                            );
                            *block_ix += 1;
                            let cell_id =
                                gpui::ElementId::NamedInteger("md-cell".into(), *block_ix as u64);
                            row.push(vec![inline_element(inline, cell_id, group, *block_ix)]);
                        }
                        Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                            *pos += 1;
                            break;
                        }
                        _ => *pos += 1,
                    }
                }
                rows.push((head, row));
            }
            _ => *pos += 1,
        }
    }
    let align_cell = |col: usize| match aligns.get(col) {
        Some(pulldown_cmark::Alignment::Center) => 1,
        Some(pulldown_cmark::Alignment::Right) => 2,
        _ => 0,
    };
    let row_count = rows.len();
    let mut table = div()
        .w_full()
        .my_1()
        .overflow_hidden()
        .flex()
        .flex_col()
        .border_1()
        .rounded_sm()
        .border_color(rgb(crate::shared::theme::BORDER));
    for (r, (head, row)) in rows.into_iter().enumerate() {
        let mut line = div()
            .w_full()
            .flex()
            .when(head, |l| l.bg(rgb(crate::shared::theme::CODE_HEADER)))
            .when(r + 1 < row_count, |l| {
                l.border_b_1()
                    .border_color(rgb(crate::shared::theme::BORDER))
            });
        for (c, cell) in row.into_iter().enumerate() {
            let mut cell_el = div().flex_1().min_w_0().px_2().py_1();
            cell_el = match align_cell(c) {
                1 => cell_el.text_align(gpui::TextAlign::Center),
                2 => cell_el.text_align(gpui::TextAlign::Right),
                _ => cell_el,
            };
            if head {
                cell_el = cell_el.font_weight(gpui::FontWeight::SEMIBOLD);
            }
            line = line.child(cell_el.children(cell));
        }
        table = table.child(line);
    }
    table.into_any_element()
}
