use crate::{
    app::root::RootView,
    terminal::{
        metrics::CellMetrics,
        pane::{SharedWriter, TermDims},
    },
};
use alacritty_terminal::{
    index::{Column, Line, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
};
use gpui::{Bounds, Context, Div, MouseButton, Pixels, Point, prelude::*};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct TerminalOwner {
    workspace: Option<String>,
    writer: SharedWriter,
}

pub(crate) fn cell_at(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
    dims: TermDims,
    offset: usize,
    cell: CellMetrics,
) -> Option<(TermPoint, Side)> {
    let x = f64::from(position.x - bounds.origin.x) as f32;
    let y = f64::from(position.y - bounds.origin.y) as f32;
    if !x.is_finite()
        || !y.is_finite()
        || bounds.size.width <= gpui::px(0.)
        || bounds.size.height <= gpui::px(0.)
        || dims.cols == 0
        || dims.rows == 0
        || !cell.width.is_finite()
        || cell.width <= 0.
        || !cell.height.is_finite()
        || cell.height <= 0.
    {
        return None;
    }
    let row = (y.max(0.) / cell.height)
        .floor()
        .min((dims.rows - 1) as f32) as i32;
    let col = (x.max(0.) / cell.width).floor().min((dims.cols - 1) as f32) as usize;
    let side = if x - col as f32 * cell.width < cell.width / 2. {
        Side::Left
    } else {
        Side::Right
    };
    Some((
        TermPoint::new(Line(row - i32::try_from(offset).ok()?), Column(col)),
        side,
    ))
}

impl RootView {
    pub(crate) fn terminal_owner(&self) -> TerminalOwner {
        TerminalOwner {
            workspace: self.term.workspace.clone(),
            writer: self.term.writer.clone(),
        }
    }

    pub(crate) fn terminal_owner_current(&self, owner: &TerminalOwner, cx: &Context<Self>) -> bool {
        owner.workspace.is_some()
            && self.term.workspace == owner.workspace
            && self.state.read(cx).active_ws_key() == owner.workspace
            && Arc::ptr_eq(&self.term.writer, &owner.writer)
    }

    fn selection_position(&self, position: Point<Pixels>) -> Option<(TermPoint, Side)> {
        let bounds = (*self.term.grid_bounds.lock().ok()?)?;
        let term = self.term.term.as_ref()?;
        cell_at(
            bounds,
            position,
            self.term.dims,
            term.grid().display_offset(),
            self.term.cell_metrics,
        )
    }

    fn update_terminal_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.term.selecting {
            return;
        }
        if let Some((point, side)) = self.selection_position(position)
            && let Some(term) = self.term.term.as_mut()
            && let Some(selection) = term.selection.as_mut()
        {
            selection.update(point, side);
            cx.notify();
        }
    }

    pub(crate) fn terminal_selection_handlers(
        &self,
        grid: gpui::Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let down_owner = self.terminal_owner();
        let move_owner = down_owner.clone();
        let up_owner = down_owner.clone();
        let out_owner = down_owner.clone();
        grid.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                if !this.terminal_owner_current(&down_owner, cx) {
                    return;
                }
                window.focus(&this.term.focus, cx);
                if let Some((point, side)) = this.selection_position(event.position)
                    && let Some(term) = this.term.term.as_mut()
                {
                    term.selection = Some(Selection::new(SelectionType::Simple, point, side));
                    this.term.selecting = true;
                    this.term.middle_anchor = None;
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(
            cx.listener(move |this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.terminal_owner_current(&move_owner, cx) {
                    this.update_terminal_selection(event.position, cx);
                }
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseUpEvent, _, cx| {
                if this.terminal_owner_current(&up_owner, cx) {
                    this.update_terminal_selection(event.position, cx);
                    this.term.selecting = false;
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |this, event: &gpui::MouseUpEvent, _, cx| {
                if this.terminal_owner_current(&out_owner, cx) {
                    this.update_terminal_selection(event.position, cx);
                    this.term.selecting = false;
                }
            }),
        )
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
