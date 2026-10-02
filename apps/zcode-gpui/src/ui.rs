//! Root layout: sidebar (all projects + sessions), header selectors, chat
//! transcript, mode bar and composer.

use crate::composer::ComposerEvent;
use crate::menus::MenuKind;
use crate::model::format_preview;
use crate::store::AppState;
use crate::theme::{BG, BORDER, CARD, MUTED, PANEL, TEXT, phase_badge};
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Entity, IntoElement, MouseButton, ParentElement,
    Render, ScrollHandle, SharedString, Styled, Window, canvas, div, prelude::*, px, rgb,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct RootView {
    pub(crate) state: Entity<AppState>,
    scroll: ScrollHandle,
    last_row_count: usize,
    last_first_row_id: u64,
    /// Set in render when new rows arrived; consumed during paint to scroll.
    scroll_pending: Arc<AtomicBool>,
    pub(crate) open_menu: Option<MenuKind>,
    pub(crate) expanded_reasonings: HashSet<u64>,
    pub(crate) expanded_tools: HashSet<u64>,
    pub(crate) session_search: String,
    pub(crate) plan_expanded: bool,
    /// Destructive action awaiting a second click ("del:<sid>", "undo:<rowId>").
    pub(crate) confirm: Option<String>,
    /// Question picks per interaction: interactionId → question index → labels.
    pub(crate) question_picks: HashMap<String, HashMap<usize, Vec<String>>>,
}

impl RootView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let composer = state.read(cx).composer.clone();
        cx.subscribe(&composer, |this, _composer, ev: &ComposerEvent, cx| {
            if matches!(ev, ComposerEvent::Submitted) {
                this.submit(cx);
            }
        })
        .detach();
        Self {
            state,
            scroll: ScrollHandle::new(),
            last_row_count: 0,
            last_first_row_id: 0,
            scroll_pending: Arc::new(AtomicBool::new(false)),
            open_menu: None,
            expanded_reasonings: HashSet::new(),
            expanded_tools: HashSet::new(),
            session_search: String::new(),
            plan_expanded: true,
            confirm: None,
            question_picks: HashMap::new(),
        }
    }

    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self
            .state
            .update(cx, |s, cx| s.composer.update(cx, |c, _ccx| c.take_text()));
        self.state.update(cx, |s, cx| s.send(&text, cx));
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let active_sid = state.active.clone();
        let is_draft = state.draft;
        let rows: Vec<crate::model::Row> = state
            .active_conversation()
            .map(|c| c.rows.values().cloned().collect())
            .unwrap_or_default();
        let running = state
            .active_conversation()
            .map(|c| c.phase_running())
            .unwrap_or(false);
        let phase = state
            .active_conversation()
            .map(|c| c.phase.clone())
            .unwrap_or_default();
        let title: SharedString = match (&active_sid, is_draft) {
            (None, true) => "New chat".into(),
            (None, false) => "ZCode".into(),
            (Some(sid), _) => {
                let title = state
                    .workspaces
                    .iter()
                    .find_map(|w| {
                        w.sessions
                            .iter()
                            .find(|s| &s.session_id == sid)
                            .map(|s| s.title.clone())
                    })
                    .unwrap_or_else(|| "Session".into());
                format_preview(&title, 60).into()
            }
        };
        let row_count = rows.len();
        let composer = state.composer.clone();
        let first_row_id = state
            .active_conversation()
            .map(|c| c.first_row_id)
            .unwrap_or(0);
        let has_more = state
            .active_conversation()
            .map(|c| c.has_more_history())
            .unwrap_or(false);
        let phase_error = state
            .active_conversation()
            .filter(|c| c.phase == "error")
            .and_then(|c| c.last_error.clone());
        let errors: Vec<String> = state.errors.iter().cloned().collect();
        let plan = state.active_conversation().and_then(|c| c.plan.clone());
        let pending_interactions = state
            .active_conversation()
            .map(|c| c.pending_interactions.clone())
            .unwrap_or_default();
        let queue = state.active_conversation().and_then(|c| c.queue.clone());
        // Row commands need the snapshot's log epoch (baseLogEpoch).
        let row_actions = state
            .active_conversation()
            .is_some_and(|c| c.log_epoch.is_some() && c.subscribed);
        let intent = state.composer_intent.clone();

        div()
            .size_full()
            .flex()
            .flex_row()
            .font_family("Segoe UI")
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            // Any click that reaches the root (i.e. outside menus) closes them;
            // menu entries stop propagation before it gets here.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.open_menu.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(self.sidebar(cx))
            // Main column
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(44.))
                            .relative()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_4()
                            .bg(rgb(PANEL))
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(div().text_size(px(13.5)).truncate().flex_1().child(title))
                            .children(plan.as_ref().map(|p| {
                                let c = p.completed_count();
                                let t = p.items.len();
                                div()
                                    .id("plan-header-badge")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(rgb(CARD))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .text_size(px(11.))
                                    .text_color(rgb(crate::theme::ACCENT))
                                    .cursor(CursorStyle::PointingHand)
                                    .child(format!("Progress {c}/{t}"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.plan_expanded = !this.plan_expanded;
                                        cx.notify();
                                    }))
                            }))
                            .children((!phase.is_empty()).then(|| phase_badge(&phase)))
                            .child(self.header_selectors(cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .children(self.error_banners(errors, phase_error, cx))
                            .child(
                                div()
                                    .id("transcript")
                                    .relative()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll)
                                    .px_4()
                                    .py_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    // History paging: rows older than the
                                    // 60-row tail window.
                                    .children(has_more.then(|| {
                                        div()
                                            .id("load-earlier")
                                            .mx_auto()
                                            .px_3()
                                            .py_1()
                                            .rounded_md()
                                            .bg(rgb(PANEL))
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .text_size(px(11.5))
                                            .text_color(rgb(MUTED))
                                            .cursor(CursorStyle::PointingHand)
                                            .hover(|s| s.bg(rgb(CARD)).text_color(rgb(TEXT)))
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, _window, cx| {
                                                    this.state.update(cx, |s, cx| {
                                                        s.fetch_earlier_rows(cx)
                                                    });
                                                },
                                            ))
                                            .child("Load earlier messages")
                                            .into_any_element()
                                    }))
                                    // scroll_to_bottom is only legal during paint, so
                                    // a paint-phase canvas consumes the pending flag.
                                    .child({
                                        let flag = self.scroll_pending.clone();
                                        let scroll = self.scroll.clone();
                                        canvas(
                                            |_, _, _| {},
                                            move |_, _, _, _| {
                                                if flag.swap(false, Ordering::Relaxed) {
                                                    scroll.scroll_to_bottom();
                                                }
                                            },
                                        )
                                        .absolute()
                                        .size_full()
                                    })
                                    .children(plan.as_ref().map(|p| {
                                        crate::turn_meta::render_plan_checklist(
                                            p,
                                            self.plan_expanded,
                                        )
                                    }))
                                    .children(self.interaction_cards(&pending_interactions, cx))
                                    .children(
                                        rows.iter()
                                            .filter_map(|r| self.transcript_row(r, row_actions, cx))
                                            .collect::<Vec<AnyElement>>(),
                                    )
                                    .when(rows.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_color(rgb(MUTED))
                                                .text_size(px(13.))
                                                .child("Start a new conversation"),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .px_3()
                            .pt_2()
                            .pb_3()
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PANEL))
                            .children(
                                queue
                                    .as_ref()
                                    .and_then(|q| crate::queue::render_queue_panel(q, cx)),
                            )
                            .children(self.intent_banner(&intent, cx))
                            .child(self.mode_bar(cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(composer)
                                    .child(crate::ui_parts::send_button(&intent, cx))
                                    .when(running, |el| el.child(crate::ui_parts::stop_button(cx))),
                            ),
                    ),
            )
            .when(
                row_count != self.last_row_count || first_row_id != self.last_first_row_id,
                |el| {
                    // Autoscroll only for appends; a history prepend must keep the
                    // viewport where it is.
                    if row_count > self.last_row_count && first_row_id == self.last_first_row_id {
                        self.scroll_pending.store(true, Ordering::Relaxed);
                    }
                    self.last_row_count = row_count;
                    self.last_first_row_id = first_row_id;
                    el
                },
            )
    }
}
