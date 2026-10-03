//! Root layout: sidebar (all projects + sessions), header selectors, chat
//! transcript, mode bar and composer.

use crate::app::dock::{DockTab, ToggleDock};
use crate::app::store::AppState;
use crate::composer::input::{Composer, ComposerEvent};
use crate::composer::menus::MenuKind;
use crate::conversation::model::format_preview;
use crate::files::pane::FilesState;
use crate::review::git::GitState;
use crate::shared::theme::{BG, BORDER, MUTED, PANEL, TEXT};
use crate::terminal::pane::{TermPane, ToggleTerminal};
use gpui::{
    Context, Entity, IntoElement, ListState, MouseButton, ParentElement, Render, SharedString,
    Styled, Window, div, prelude::*, px, rgb,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Reading width of the transcript and composer (desktop centers both).
pub(crate) const CONTENT_WIDTH: f32 = 800.;

pub struct RootView {
    pub(crate) state: Entity<AppState>,
    /// Virtualized transcript state (see transcript/list.rs).
    pub(crate) list_state: ListState,
    pub(crate) row_index: Vec<u64>,
    pub(crate) last_bounds: (u64, u64),
    /// Session the current row index belongs to (row ids repeat across
    /// conversations, so a switch must force a list reset).
    pub(crate) last_session: Option<String>,
    pub(crate) follow_bottom: Arc<AtomicBool>,
    pub(crate) last_row_count: usize,
    pub(crate) last_first_row_id: u64,
    pub(crate) open_menu: Option<MenuKind>,
    pub(crate) expanded_reasonings: HashSet<u64>,
    pub(crate) expanded_tools: HashSet<u64>,
    pub(crate) session_search: String,
    pub(crate) plan_expanded: bool,
    /// Destructive action awaiting a second click ("del:<sid>", "undo:<rowId>").
    pub(crate) confirm: Option<String>,
    /// Question picks per interaction: interactionId → question index → labels.
    pub(crate) question_picks: HashMap<String, HashMap<usize, Vec<String>>>,
    // --- M3 tool panes ---
    pub(crate) dock_open: bool,
    pub(crate) dock_tab: DockTab,
    pub(crate) term_open: bool,
    pub(crate) git: GitState,
    pub(crate) commit_input: Entity<Composer>,
    pub(crate) files: FilesState,
    pub(crate) term: TermPane,
    /// Phase of the last render; a completed turn refreshes git status.
    last_phase: String,
    /// Auto-opened the dock for the current turn's plan (once per turn).
    auto_opened_this_turn: bool,
    last_plan_present: bool,
    pub(crate) middle_scroll: Option<crate::transcript::scroll::MiddleScrollState>,
    /// An autoscroll `on_next_frame` callback is queued (see transcript/scroll.rs).
    pub(crate) autoscroll_frame_pending: bool,
    pub(crate) quickpick_open: bool,
    pub(crate) quickpick_query: String,
    pub(crate) quickpick_selected: usize,
    pub(crate) os_lifecycle: crate::app::os_lifecycle::OsLifecycleState,
    pub(crate) plugin_segment: crate::app::plugin_pane::PluginSegment,
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
        let (list_state, follow_bottom) = crate::transcript::list::new_list_state();
        let commit_input = cx.new(|cx| Composer::new_single_line("Commit message", cx));
        cx.subscribe(&commit_input, |this, _composer, ev: &ComposerEvent, cx| {
            if matches!(ev, ComposerEvent::Submitted) {
                this.do_commit(cx);
            }
        })
        .detach();
        Self {
            state,
            list_state,
            row_index: Vec::new(),
            last_bounds: (0, 0),
            last_session: None,
            follow_bottom,
            last_row_count: 0,
            last_first_row_id: 0,
            open_menu: None,
            expanded_reasonings: HashSet::new(),
            expanded_tools: HashSet::new(),
            session_search: String::new(),
            plan_expanded: false,
            confirm: None,
            question_picks: HashMap::new(),
            commit_input,
            dock_open: false,
            dock_tab: DockTab::Review,
            term_open: false,
            git: GitState::default(),
            files: FilesState::default(),
            term: TermPane::new(cx.focus_handle()),
            last_phase: String::new(),
            auto_opened_this_turn: false,
            last_plan_present: false,
            middle_scroll: None,
            autoscroll_frame_pending: false,
            quickpick_open: false,
            quickpick_query: String::new(),
            quickpick_selected: 0,
            os_lifecycle: crate::app::os_lifecycle::OsLifecycleState::default(),
            plugin_segment: crate::app::plugin_pane::PluginSegment::Public,
        }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let active_sid = state.active.clone();
        let is_draft = state.draft;
        let row_count = state.active_conversation().map_or(0, |c| c.rows.len());
        let running = state
            .active_conversation()
            .map(|c| c.phase_running())
            .unwrap_or(false);
        let phase = state
            .active_conversation()
            .map(|c| c.phase.clone())
            .unwrap_or_default();
        let title: SharedString = match (&active_sid, is_draft) {
            (None, _) => "New task".into(),
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

        crate::app::os_lifecycle::sync_window_state(
            window,
            &mut self.os_lifecycle.last_saved_bounds,
        );
        crate::app::os_lifecycle::sync_keep_awake(&mut self.os_lifecycle.keep_awake, running);
        crate::app::os_lifecycle::check_turn_completion_notification(
            &self.last_phase,
            &phase,
            &title,
            window.is_window_active(),
        );
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
        let intent = state.composer_intent.clone();
        // Reconcile the virtual list with the model before building elements.
        self.sync_list(row_count, first_row_id, cx);
        let rows_empty = row_count == 0;
        // Auto-refresh the Review pane when a turn finishes writing files.
        if self.last_phase != phase {
            if crate::conversation::model::phase_is_active(&phase)
                && !crate::conversation::model::phase_is_active(&self.last_phase)
            {
                self.auto_opened_this_turn = false;
            }
            if phase == "completedSuccess" {
                // Collapse the plan capsule once its work is done; the user
                // can re-expand it. Also refresh the Review pane if open.
                self.plan_expanded = false;
                if self.dock_open {
                    self.refresh_git(cx);
                }
            }
            self.last_phase = phase.clone();
        }
        // Desktop parity: a turn with a plan opens the side panel once, so the
        // progress capsule is visible without hunting for it. A manual close
        // is respected for the rest of the turn.
        let plan_present = plan.as_ref().is_some_and(|p| !p.items.is_empty());
        if plan_present && !self.last_plan_present && !self.auto_opened_this_turn {
            self.dock_open = true;
            self.dock_tab = crate::app::dock::DockTab::Review;
            self.auto_opened_this_turn = true;
            self.on_dock_tab(crate::app::dock::DockTab::Review, cx);
        }
        self.last_plan_present = plan_present;

        div()
            .size_full()
            .flex()
            .flex_row()
            .font_family("Segoe UI")
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .key_context("Root")
            .on_action(cx.listener(|this, _: &ToggleDock, _, cx| {
                this.dock_open = !this.dock_open;
                if this.dock_open {
                    this.on_dock_tab(this.dock_tab, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleTerminal, window, cx| {
                this.term_open = !this.term_open;
                if this.term_open {
                    this.ensure_term(cx);
                    window.focus(&this.term.focus);
                }
                cx.notify();
            }))
            .on_action(
                cx.listener(|this, _: &crate::app::quickpick::ToggleQuickPick, _, cx| {
                    this.quickpick_open = !this.quickpick_open;
                    this.quickpick_query.clear();
                    this.quickpick_selected = 0;
                    cx.notify();
                }),
            )
            .on_action(cx.listener(
                |_this, _: &crate::app::quickpick::SwitchThemeAction, _, cx| {
                    let next = match crate::shared::theme::theme_mode() {
                        crate::shared::theme::ThemeMode::ZaiLight => {
                            crate::shared::theme::ThemeMode::ZaiDark
                        }
                        _ => crate::shared::theme::ThemeMode::ZaiLight,
                    };
                    crate::shared::theme::set_theme_mode(next);
                    cx.notify();
                },
            ))
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if this.quickpick_open && crate::app::quickpick::handle_quickpick_key(this, ev, cx)
                {
                    cx.stop_propagation();
                }
            }))
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
            // Autoscroll gestures are window-wide: any other button cancels
            // (capture phase, so buttons that stop propagation still cancel),
            // and move/release keep tracking outside the transcript.
            .capture_any_mouse_down(cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                if ev.button != MouseButton::Middle {
                    this.cancel_middle_scroll(cx);
                }
            }))
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _, cx| {
                this.handle_middle_move(ev, cx);
            }))
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, ev: &gpui::MouseUpEvent, _, cx| {
                    this.handle_middle_up(ev, cx);
                }),
            )
            .child(self.sidebar(cx))
            // Main column: a rounded panel inset from the window edge, as in
            // the desktop workspace layout.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .my_1()
                    .mr_1()
                    .rounded_lg()
                    .overflow_hidden()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .child(self.main_header(title, plan.as_ref(), &phase, cx))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .children(self.error_banners(errors, phase_error, cx))
                            // Pinned strip above the transcript: history paging,
                            // plan checklist and pending interaction cards.
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(CONTENT_WIDTH))
                                    .mx_auto()
                                    .px_6()
                                    .pt_2()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .children(self.load_earlier_btn(has_more, cx))
                                    .children(self.interaction_cards(&pending_interactions, cx)),
                            )
                            .child(if rows_empty {
                                div()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(MUTED))
                                    .text_size(px(13.))
                                    .child("Start a new conversation")
                                    .into_any_element()
                            } else {
                                self.transcript_list(cx)
                            })
                            // Floating tool dock draws over the transcript.
                            .when(self.dock_open, |el| el.child(self.dock_pane(cx))),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_w(px(CONTENT_WIDTH + 24.))
                            .mx_auto()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .px_3()
                            .pt_1()
                            .pb_3()
                            .children(queue.as_ref().and_then(|q| {
                                crate::conversation::queue::render_queue_panel(q, cx)
                            }))
                            .children(self.intent_banner(&intent, cx))
                            .child(self.composer_card(composer, running, cx)),
                    )
                    .when(self.term_open, |el| el.child(self.term_drawer(window, cx))),
            )
            .children(
                self.quickpick_open
                    .then(|| crate::app::quickpick::render_quickpick_modal(self, window, cx)),
            )
    }
}
