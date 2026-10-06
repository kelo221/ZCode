//! Smaller pieces of the root layout: error banners, the composer intent
//! banner (editing a message / renaming a session) and the composer card.

use crate::app::root::RootView;
use crate::composer::input::Composer;
use crate::conversation::msg_actions::ComposerIntent;
use crate::shared::theme::ui_size;
use crate::shared::theme::{
    AMBER, BORDER, CARD, CARD_HOVER, DANGER, I_ARROW_UP, I_STOP, MUTED, PANEL, PRIMARY, TEXT, icon,
};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, Div, Entity, ParentElement, Stateful, Styled,
    div, prelude::*, px,
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
    pub(crate) fn queue_panel(
        &self,
        queue: Option<&crate::conversation::queue::QueueState>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        queue.and_then(|queue| {
            crate::conversation::queue::render_queue_panel(queue, &self.state, cx)
        })
    }

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
                            .text_size(px(ui_size(12.)))
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
                    .text_size(px(ui_size(12.)))
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
                        .text_size(px(ui_size(11.5)))
                        .text_color(rgb(AMBER))
                        .child(label),
                )
                .child(
                    div()
                        .id("intent-cancel")
                        .px_1p5()
                        .text_size(px(ui_size(11.5)))
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
                .text_size(px(ui_size(11.5)))
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
            .on_click(cx.listener(|this, event: &ClickEvent, _window, cx| {
                let trigger = if crate::composer::delivery::primary_modifier(&event.modifiers()) {
                    crate::composer::delivery::SubmitTrigger::ModifiedPointer
                } else {
                    crate::composer::delivery::SubmitTrigger::Ordinary
                };
                this.state
                    .update(cx, |s, cx| s.submit_composer_with_trigger(trigger, cx));
            }));
        let stop = round_button("stop-btn", I_STOP, CARD_HOVER, TEXT)
            .hover(|s| s.bg(rgb(BORDER)))
            .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                this.state.update(cx, |s, cx| s.stop(cx));
            }));

        let query = crate::composer::references::reference_query(composer.read(cx).text())
            .map(|(kind, _, _)| kind);
        if let Some(kind) = query {
            self.state.update(cx, |state, cx| {
                state.ensure_reference_catalog(kind, false, cx)
            });
        }
        if crate::composer::slash::filter_slash_commands(&[], composer.read(cx).text()).is_some() {
            self.state
                .update(cx, |state, cx| state.ensure_slash_catalog(false, cx));
        }
        let suggestions = self.autocomplete_suggestions(cx);
        let popup = suggestions.map(|s| self.render_autocomplete_popup(&s, cx));
        let att_tray = Self::render_attachment_tray(&composer, cx);
        let attach_btn = Self::render_attach_button(cx);
        let view = cx.weak_entity();
        let held_confirmation = self.held_queue_confirmation(cx);

        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .children(held_confirmation)
            .children(self.image_upload_feedback(cx))
            .children(self.reference_feedback(cx))
            .children(self.slash_catalog_feedback(cx))
            .children(popup)
            .child(
                div()
                    .w_full()
                    .min_w_0()
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
                    .on_children_prepainted(move |bounds, _, cx| {
                        if let Some(bounds) = bounds.last() {
                            let compact = bounds.size.width <= px(480.);
                            let _ = view.update(cx, |this, cx| {
                                if this.composer_compact != compact {
                                    this.composer_compact = compact;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .children(att_tray)
                    .child(composer)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .w_full()
                            .min_w_0()
                            .flex_wrap()
                            .items_center()
                            .gap_1()
                            .child(attach_btn)
                            .child(self.mode_menu(cx))
                            .children(self.composer_activity(cx))
                            .child(div().flex_1())
                            .child(self.composer_selectors(cx))
                            .when(running, |el| el.child(stop))
                            .child({
                                let wrapper = div().child(send);
                                #[cfg(test)]
                                let wrapper = crate::app::test_support::track_children(
                                    wrapper,
                                    vec!["send-btn".into()],
                                );
                                wrapper
                            }),
                    ),
            )
    }

    pub(crate) fn autocomplete_suggestions(
        &self,
        cx: &Context<Self>,
    ) -> Option<Vec<crate::composer::autocomplete::AutocompleteSuggestion>> {
        let state = self.state.read(cx);
        let text = state.composer.read(cx).text().to_string();
        let slash = state.active_slash_commands().unwrap_or_default();
        let sessions = state.active_sessions();
        let ws_path = state.active_workspace_path();
        let mut references = self.reference_suggestions(&text, cx);
        references.extend(
            crate::composer::autocomplete::detect_autocomplete(
                &text,
                slash,
                &sessions,
                ws_path.as_deref(),
            )
            .unwrap_or_default(),
        );
        references.truncate(8);
        (!references.is_empty()).then_some(references)
    }

    /// Filesystem path of the active workspace (git, terminal, file tree).
    pub(crate) fn active_workspace_path(&self, cx: &Context<Self>) -> Option<std::path::PathBuf> {
        self.state.read(cx).active_workspace_path()
    }

    pub(crate) fn submit(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| state.submit_composer(cx));
    }
}
