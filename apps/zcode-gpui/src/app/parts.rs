//! Smaller pieces of the root layout: error banners, the composer intent
//! banner (editing a message / renaming a session) and the composer card.

use crate::app::root::RootView;
use crate::composer::input::Composer;
use crate::conversation::msg_actions::ComposerIntent;
use crate::conversation::turn_meta::PlanState;
use crate::shared::theme::{
    ACCENT, AMBER, BORDER, CARD, CARD_HOVER, DANGER, I_ARROW_UP, I_STOP, MUTED, PANEL, PRIMARY,
    TEXT, icon, phase_badge,
};
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Div, Entity, ParentElement, SharedString,
    Stateful, Styled, div, prelude::*, px, rgb,
};

fn banner() -> Div {
    div()
        .mx_3()
        .mt_2()
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(rgb(0x3d2323))
        .border_1()
        .border_color(rgb(DANGER))
}

impl RootView {
    /// Dismissable error banners (newest first) plus the backend turn error.
    pub(crate) fn error_banners(
        &self,
        errors: Vec<String>,
        phase_error: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = errors
            .into_iter()
            .rev()
            .enumerate()
            .map(|(i, msg)| {
                banner()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(rgb(DANGER))
                            .truncate()
                            .child(msg),
                    )
                    .child(
                        div()
                            .id(("err", i))
                            .px_1p5()
                            .cursor(CursorStyle::PointingHand)
                            .text_color(rgb(MUTED))
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                                this.state.update(cx, |s, _cx| s.dismiss_error(i));
                                cx.notify();
                            }))
                            .child("✕"),
                    )
                    .into_any_element()
            })
            .collect();
        if let Some((code, message)) = phase_error {
            out.push(
                banner()
                    .text_size(px(12.))
                    .text_color(rgb(DANGER))
                    .child(format!("{code}: {message}"))
                    .into_any_element(),
            );
        }
        out
    }

    /// "Editing message" / "Renaming session" strip above the composer.
    pub(crate) fn intent_banner(
        &self,
        intent: &ComposerIntent,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let label = match intent {
            ComposerIntent::Send => return None,
            ComposerIntent::Edit { .. } => {
                "Editing message: Enter resends it and rewinds the conversation to this point"
            }
            ComposerIntent::Rename { .. } => "Renaming session: Enter saves the new title",
        };
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(AMBER))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.5))
                        .text_color(rgb(AMBER))
                        .child(label),
                )
                .child(
                    div()
                        .id("intent-cancel")
                        .px_1p5()
                        .text_size(px(11.5))
                        .text_color(rgb(MUTED))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.state.update(cx, |s, cx| s.cancel_intent(cx));
                        }))
                        .child("Cancel"),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn load_earlier_btn(
        &self,
        has_more: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !has_more {
            return None;
        }
        Some(
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
                .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.state.update(cx, |s, cx| s.fetch_earlier_rows(cx));
                }))
                .child("Load earlier messages")
                .into_any_element(),
        )
    }
}

fn round_button(id: &'static str, glyph: char, bg: u32, fg: u32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(rgb(bg))
        .cursor(CursorStyle::PointingHand)
        .child(icon(glyph, 12., fg))
}

impl RootView {
    /// Desktop-style composer: one rounded card with the input on top and
    /// mode / model / thinking pickers plus the round send button below.
    pub(crate) fn composer_card(
        &mut self,
        composer: Entity<Composer>,
        running: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let send = round_button("send-btn", I_ARROW_UP, PRIMARY, 0x000000)
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| this.submit(cx)));
        let stop = round_button("stop-btn", I_STOP, CARD_HOVER, TEXT)
            .hover(|s| s.bg(rgb(BORDER)))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.update(cx, |s, cx| s.stop(cx));
            }));

        let suggestions = self.autocomplete_suggestions(cx);
        let popup = suggestions.map(|s| self.render_autocomplete_popup(&s, cx));
        let att_tray = Self::render_attachment_tray(&composer, cx);
        let attach_btn = Self::render_attach_button(cx);

        div().w_full().flex().flex_col().children(popup).child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .px_3()
                .pt_3()
                .pb_2()
                .rounded_xl()
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(BORDER))
                .on_drop(
                    cx.listener(|this, paths: &gpui::ExternalPaths, _window, cx| {
                        this.attach_files(paths.paths().to_vec(), cx);
                    }),
                )
                .children(att_tray)
                .child(composer)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(self.mode_menu(cx))
                        .child(attach_btn)
                        .child(div().flex_1())
                        .child(self.composer_selectors(cx))
                        .when(running, |el| el.child(stop))
                        .child(send),
                ),
        )
    }

    pub(crate) fn autocomplete_suggestions(
        &self,
        cx: &Context<Self>,
    ) -> Option<Vec<crate::composer::autocomplete::AutocompleteSuggestion>> {
        let state = self.state.read(cx);
        let text = state.composer.read(cx).text().to_string();
        let mut slash = crate::composer::slash::builtin_slash_commands();
        if let Some(cfg) = state.active_workspace_config() {
            for cmd in cfg.slash_commands() {
                if !slash.iter().any(|s| s.name == cmd.name) {
                    slash.push(cmd);
                }
            }
        }
        let sessions = state.active_sessions();
        let ws_path = state.active_workspace_path();
        crate::composer::autocomplete::detect_autocomplete(
            &text,
            &slash,
            &sessions,
            ws_path.as_deref(),
        )
    }

    /// Filesystem path of the active workspace (git, terminal, file tree).
    pub(crate) fn active_workspace_path(&self, cx: &Context<Self>) -> Option<std::path::PathBuf> {
        self.state.read(cx).active_workspace_path()
    }

    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        let (text, attachments) = self.state.update(cx, |s, cx| {
            s.composer
                .update(cx, |c, _ccx| (c.take_text(), c.take_attachments()))
        });
        self.state
            .update(cx, |s, cx| s.send_with_attachments(&text, attachments, cx));
    }

    /// Header bar with conversation title, progress counter, tools & terminal toggles.
    pub(crate) fn main_header(
        &self,
        title: SharedString,
        plan: Option<&PlanState>,
        phase: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .h(px(46.))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_5()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .truncate()
                    .flex_1()
                    .child(title),
            )
            .children(plan.map(|p| {
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
                    .text_color(rgb(ACCENT))
                    .cursor(CursorStyle::PointingHand)
                    .child(format!("Progress {c}/{t}"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dock_open = true;
                        this.dock_tab = crate::app::dock::DockTab::Review;
                        this.plan_expanded = !this.plan_expanded;
                        cx.notify();
                    }))
            }))
            .children(phase_badge(phase))
            .child(
                div()
                    .id("dock-toggle")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(11.))
                    .text_color(rgb(if self.dock_open { ACCENT } else { MUTED }))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dock_open = !this.dock_open;
                        if this.dock_open {
                            this.on_dock_tab(this.dock_tab, cx);
                        }
                        cx.notify();
                    }))
                    .child("Tools")
                    .child(icon('◂', 8., MUTED)),
            )
            .child(
                div()
                    .id("term-toggle")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(11.))
                    .text_color(rgb(if self.term_open { ACCENT } else { MUTED }))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.term_open = !this.term_open;
                        if this.term_open {
                            this.ensure_term(cx);
                            window.focus(&this.term.focus);
                        }
                        cx.notify();
                    }))
                    .child("Terminal")
                    .child(icon('▤', 8., MUTED)),
            )
    }
}
