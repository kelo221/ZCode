//! Composer rendering: the content is one wrapped `StyledText` (selection
//! and IME composition as highlights) with the caret painted from its
//! `TextLayout`, so long lines wrap inside the input and the caret follows
//! the wrapped position. Mouse down places the caret, Shift+click and drag
//! extend the selection.

use crate::composer::input::Composer;
use crate::shared::theme::{MUTED, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    Bounds, Context, CursorStyle, HighlightStyle, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, Point, Render, ScrollHandle, StyledText, TextLayout, UnderlineStyle, Window, canvas,
    div, fill, prelude::*, px, size,
};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct TextViewState {
    /// Layout of the last rendered content; `None` while the input is empty.
    layout: Option<TextLayout>,
    /// Set once that layout has been painted; until then it cannot be queried.
    painted: Rc<Cell<bool>>,
    /// Caret rectangle from the last paint (IME candidate window anchor).
    pub(crate) caret_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: bool,
    /// Viewport of the height-capped input (Desktop `max-h-40 overflow-y-auto`).
    pub(crate) scroll: ScrollHandle,
    /// Caret and content length at the last render; a change scrolls the caret into view.
    last_caret: usize,
    last_len: usize,
}

/// Desktop LexicalChatInput `max-h-40`: the input grows to 160px, then scrolls.
pub(crate) const COMPOSER_MAX_HEIGHT: f32 = 160.;

/// Vertical scroll offset that brings `caret` inside `viewport`, if it is outside.
fn reveal_offset(
    caret: Bounds<Pixels>,
    viewport: Bounds<Pixels>,
    offset: Point<Pixels>,
) -> Option<Point<Pixels>> {
    let y = if caret.bottom() > viewport.bottom() {
        offset.y - (caret.bottom() - viewport.bottom())
    } else if caret.top() < viewport.top() {
        offset.y + (viewport.top() - caret.top())
    } else {
        return None;
    };
    Some(Point::new(offset.x, y.min(px(0.))))
}

impl Composer {
    /// Byte index under a window position, snapped to a char boundary and
    /// out of any chip.
    fn index_at(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self
            .view
            .layout
            .as_ref()
            .filter(|_| self.view.painted.get())?;
        let mut ix = layout
            .index_for_position(position)
            .unwrap_or_else(|ix| ix)
            .min(self.content.len());
        while !self.content.is_char_boundary(ix) {
            ix -= 1;
        }
        Some(self.chips.snap(ix))
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(ix) = self.index_at(ev.position) else {
            return;
        };
        if ev.click_count >= 3 {
            self.select_all(cx);
            return;
        }
        self.press_at(ix, ev.modifiers.shift, cx);
        self.view.dragging = true;
    }

    /// Pointer press at byte `ix`: Shift extends from the current caret,
    /// otherwise the caret moves there and the selection collapses.
    pub(crate) fn press_at(&mut self, ix: usize, shift: bool, cx: &mut Context<Self>) {
        // 原实现先清空 anchor 再以 extend=true 移动，get_or_insert 会把旧光标位置当作锚点，
        // 普通单击因此选中了“旧光标→点击处”，光标看起来没有移到新位置。
        // 非 Shift 按下应只移动光标；拖动时再以按下位置为锚点扩展选区。
        self.move_to(ix, shift, cx);
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.view.dragging || ev.pressed_button != Some(MouseButton::Left) {
            self.view.dragging = false;
            return;
        }
        if let Some(ix) = self.index_at(ev.position)
            && ix != self.caret
        {
            self.move_to(ix, true, cx);
        }
    }

    fn marked_bytes(&self) -> Option<std::ops::Range<usize>> {
        let m = self.marked_utf16.clone()?;
        Some(self.byte_from_utf16(m.start)?..self.byte_from_utf16(m.end)?)
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.content.is_empty();
        let focused = self.focus.is_focused(window);
        let entity = cx.entity();
        let focus = self.focus.clone();
        let caret = self.caret;
        let painted = Rc::new(Cell::new(false));
        self.view.painted = painted.clone();
        let caret_slot = self.view.caret_bounds.clone();
        let scroll = self.view.scroll.clone();
        let reveal = caret != self.view.last_caret || self.content.len() != self.view.last_len;
        self.view.last_caret = caret;
        self.view.last_len = self.content.len();
        let notify_target = entity.clone();

        let mut highlights = Vec::new();
        let marked = self.marked_bytes().filter(|r| !r.is_empty());
        if marked.is_none()
            && let Some(sel) = self.selection()
        {
            let mut bg = rgb(crate::shared::theme::ACCENT);
            bg.a = 0.35;
            highlights.push((
                sel,
                HighlightStyle {
                    background_color: Some(bg.into()),
                    ..Default::default()
                },
            ));
        }
        if let Some(marked) = marked {
            highlights.push((
                marked,
                HighlightStyle {
                    underline: Some(UnderlineStyle {
                        thickness: px(1.),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ));
        }
        let text =
            (!empty).then(|| StyledText::new(self.content.clone()).with_highlights(highlights));
        let layout = text.as_ref().map(|t| t.layout().clone());
        self.view.layout = layout.clone();

        let placeholder = if self.single_line {
            self.placeholder
        } else if self.new_task {
            crate::shared::i18n::t("chat.placeholder.newTask")
        } else {
            crate::shared::i18n::t("chat.placeholder.followUpAsk")
        };

        div()
            .id("composer-input")
            .key_context("Composer")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .w_full()
            .min_w_0()
            .min_h(px(44.))
            .px_1()
            .text_size(px(ui_size(14.)))
            .text_color(rgb(TEXT))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.view.dragging = false),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .id("composer-scroll")
                    .w_full()
                    .min_w_0()
                    .max_h(px(ui_size(COMPOSER_MAX_HEIGHT)))
                    .overflow_y_scroll()
                    .track_scroll(&self.view.scroll)
                    .child(
                        div()
                            .id("composer-canvas-host")
                            .relative()
                            .w_full()
                            .min_w_0()
                            .map(|el| match text {
                                Some(text) => el.child(div().w_full().min_w_0().child(text)),
                                None => el.child(div().text_color(rgb(MUTED)).child(placeholder)),
                            })
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, cx| {
                                        window.handle_input(
                                            &focus,
                                            gpui::ElementInputHandler::new(bounds, entity.clone()),
                                            cx,
                                        );
                                        // Prepaint has run for the text by now, so its layout can be read.
                                        let (origin, line_h) = match &layout {
                                            Some(l) => {
                                                painted.set(true);
                                                let at = l
                                                    .position_for_index(caret)
                                                    .unwrap_or(bounds.origin);
                                                (at, l.line_height())
                                            }
                                            None => (bounds.origin, window.line_height()),
                                        };
                                        let caret_rect = Bounds::new(origin, size(px(1.5), line_h));
                                        caret_slot.set(Some(caret_rect));
                                        if reveal
                                            && let Some(next) = reveal_offset(
                                                caret_rect,
                                                scroll.bounds(),
                                                scroll.offset(),
                                            )
                                        {
                                            // 输入框限高后，键入/粘贴/方向键会把光标移出可视区；
                                            // 绘制期 refresh 无效，只能下一帧通知重绘以应用新偏移。
                                            scroll.set_offset(next);
                                            let target = notify_target.clone();
                                            window.on_next_frame(move |_, cx| {
                                                target.update(cx, |_, cx| cx.notify())
                                            });
                                        }
                                        if focused {
                                            window.paint_quad(fill(caret_rect, rgb(TEXT)));
                                        }
                                    },
                                )
                                // 缺少 top/left 时绝对定位元素落在静态位置，即占位文字的下一行；
                                // 空输入的光标取 canvas 原点，因此会画在占位文字下方而非其起点。
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
#[path = "text_view_tests.rs"]
mod tests;
