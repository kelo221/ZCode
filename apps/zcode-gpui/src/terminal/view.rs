//! Terminal UI drawer: expandable bottom drawer matching desktop
//! AnimatedTerminalPanel parity. Renders colored grid rows, provides focus
//! handling, and auto-reports size changes via paint-phase probe.

use crate::app::root::RootView;
use crate::shared::theme::{BORDER, CODE_BG, CODE_BORDER, HOVER, MUTED, PANEL, TEXT};
use crate::terminal::grid::{self, GridRun, TERM_FG};
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, MouseButton, ParentElement, Styled, div,
    prelude::*, px, rgb,
};
use std::sync::{Arc, Mutex};

const FONT_SIZE: f32 = 11.5;

impl RootView {
    /// Render the terminal as an expandable bottom drawer.
    pub(crate) fn term_drawer(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.term.workspace.is_none() {
            return div()
                .p_3()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child("No workspace selected")
                .into_any_element();
        }
        let dims = self.term.dims;
        let rows = self
            .term
            .term
            .as_ref()
            .map(|term| grid::grid_rows(term.renderable_content(), TERM_FG, CODE_BG))
            .unwrap_or_default();
        let focused = self.term.focus.is_focused(window);

        let mut grid = div()
            .id("term-grid")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p_1p5()
            .bg(rgb(CODE_BG))
            .font_family(crate::shared::theme::MONO_FONT)
            .text_size(px(FONT_SIZE))
            .text_color(rgb(TERM_FG))
            .key_context("Terminal")
            .track_focus(&self.term.focus)
            .cursor(if self.term.middle_anchor.is_some() {
                CursorStyle::ResizeUpDown
            } else {
                CursorStyle::IBeam
            })
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.term.focus, cx);
            }))
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _window, cx| {
                this.term_key(&ev.keystroke, cx);
            }))
            .on_scroll_wheel(
                cx.listener(|this, ev: &gpui::ScrollWheelEvent, _window, cx| {
                    let delta = ev.delta.pixel_delta(px(20.));
                    let lines = (delta.y / px(crate::terminal::pane::CELL_H)).round() as i32;
                    if lines != 0
                        && let Some(term) = this.term.term.as_mut()
                    {
                        term.scroll_display(alacritty_terminal::grid::Scroll::Delta(lines));
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, ev: &gpui::MouseDownEvent, _window, cx| {
                    this.term.middle_anchor = Some(ev.position);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _ev: &gpui::MouseUpEvent, _window, cx| {
                    this.term.middle_anchor = None;
                    cx.notify();
                }),
            )
            // Releasing outside the grid must also end the drag, or the next
            // hover would keep scrolling with a stale anchor.
            .on_mouse_up_out(
                MouseButton::Middle,
                cx.listener(|this, _ev: &gpui::MouseUpEvent, _window, cx| {
                    if this.term.middle_anchor.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _window, cx| {
                if let Some(anchor) = this.term.middle_anchor {
                    let dy = ev.position.y - anchor.y;
                    let lines = -(dy / px(crate::terminal::pane::CELL_H)).round() as i32;
                    if lines != 0
                        && let Some(term) = this.term.term.as_mut()
                    {
                        term.scroll_display(alacritty_terminal::grid::Scroll::Delta(lines));
                        this.term.middle_anchor = Some(ev.position);
                        cx.notify();
                    }
                }
            }))
            .children(rows.into_iter().map(term_row));
        if !focused {
            grid = grid.child(
                div()
                    .p_1()
                    .text_size(px(10.5))
                    .text_color(rgb(MUTED))
                    .child("Click to focus the terminal"),
            );
        }

        div()
            .h(px(240.))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            // Header bar of the bottom terminal drawer.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(28.))
                    .border_b_1()
                    .border_color(rgb(CODE_BORDER))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(TEXT))
                            .child("Terminal"),
                    )
                    .child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(
                        if self.term.exited {
                            "process exited — reopen to restart".to_string()
                        } else {
                            format!("{}×{} — local shell", dims.cols, dims.rows)
                        },
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("term-drawer-close")
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.term_open = false;
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
            .child(
                div().relative().flex_1().min_h_0().child(grid).child(
                    div()
                        .absolute()
                        .inset_0()
                        .child(resize_probe(&self.term.pending_size)),
                ),
            )
            .into_any_element()
    }
}

fn term_row(runs: Vec<GridRun>) -> gpui::Div {
    let mut row = div().flex();
    if runs.is_empty() {
        return row.child(" ");
    }
    for run in runs {
        let mut span = div().text_color(rgb(run.fg)).child(run.text.clone());
        if let Some(bg) = run.bg {
            span = span.bg(rgb(bg));
        }
        if run.bold {
            span = span.font_weight(gpui::FontWeight::BOLD);
        }
        if run.italic {
            span = span.italic();
        }
        row = row.child(span);
    }
    row
}

/// Paint-phase probe recording the grid pixel size for PTY resize (applied on
/// the next poll tick — entities cannot be mutated during paint).
fn resize_probe(pending: &Arc<Mutex<Option<(f32, f32)>>>) -> impl IntoElement {
    let pending = pending.clone();
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, _, _| {
            if let Ok(mut p) = pending.lock() {
                *p = Some((
                    f64::from(bounds.size.width) as f32,
                    f64::from(bounds.size.height) as f32,
                ));
            }
        },
    )
}
