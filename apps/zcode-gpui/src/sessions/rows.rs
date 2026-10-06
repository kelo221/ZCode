//! Sidebar session rows (split from sessions/sidebar.rs for the 400-line
//! cap): one thread row with its rename/delete controls, plus the
//! progressive "Load more" row.

use crate::conversation::model::{format_preview, phase_is_active};
use crate::sessions::items::{ROW_GROUP, format_relative_time, nav_row};
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, DANGER, I_CLOSE, I_EDIT, MUTED, TEXT, icon};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*, px};

impl crate::app::root::RootView {
    pub(crate) fn session_row(
        &self,
        ws_key: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (sid, full_title, title, phase, is_active, rel_time) = {
            let state = self.state.read(cx);
            let session = state.ws(ws_key)?.sessions.get(index)?;
            (
                session.session_id.clone(),
                session.title.clone(),
                format_preview(&session.title, 40),
                session.phase.clone(),
                state.active.as_deref() == Some(session.session_id.as_str()),
                format_relative_time(session.last_activity_at),
            )
        };
        let ws_key = ws_key.to_string();
        let confirming = self.confirm.as_deref() == Some(format!("del:{sid}").as_str());
        let controls = self.session_controls(&ws_key, &sid, &full_title, confirming, cx);
        let dot = if phase_is_active(&phase) {
            Some(ACCENT)
        } else if phase == "error" {
            Some(DANGER)
        } else {
            None
        };
        Some(
            nav_row(SharedString::from(sid.clone()), is_active)
                .group(ROW_GROUP)
                .pl(px(34.))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.close_settings(window, cx);
                    this.state
                        .update(cx, |s, cx| s.select_session(&ws_key, &sid, cx));
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(TEXT))
                        .child(SharedString::from(title)),
                )
                .children(dot.map(|c| div().size(px(6.)).rounded_full().bg(rgb(c))))
                // Time normally; rename/delete on hover (always while confirming).
                // The swap uses opacity, not display: gpui re-reads hover state
                // at paint, and a display change between prepaint and paint
                // panics ("must call prepaint before paint").
                .child(if confirming {
                    div().child(controls)
                } else {
                    div()
                        .relative()
                        .min_w(px(44.))
                        .flex()
                        .justify_end()
                        .child(
                            div()
                                .text_size(px(ui_size(12.)))
                                .text_color(rgb(MUTED))
                                .group_hover(ROW_GROUP, |s| s.opacity(0.))
                                .child(rel_time),
                        )
                        .child(
                            div()
                                .absolute()
                                .right_0()
                                .top_0()
                                .bottom_0()
                                .flex()
                                .items_center()
                                .opacity(0.)
                                .group_hover(ROW_GROUP, |s| s.opacity(1.))
                                .child(controls),
                        )
                })
                .into_any_element(),
        )
    }

    /// Rename + delete. Delete goes to the workspace that owns the session
    /// and needs a second click to confirm.
    fn session_controls(
        &self,
        ws_key: &str,
        sid: &str,
        title: &str,
        confirming: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let btn = |id: String, hover: u32| {
            div()
                .id(SharedString::from(id))
                .px_1()
                .text_size(px(ui_size(12.)))
                .text_color(rgb(MUTED))
                .hover(move |s| s.text_color(rgb(hover)))
                .cursor_pointer()
        };
        let (ws, id, t) = (ws_key.to_string(), sid.to_string(), title.to_string());
        if confirming {
            let (ws_c, id_c) = (ws.clone(), id.clone());
            return div()
                .flex()
                .items_center()
                .child(
                    btn(format!("del-ok-{id}"), DANGER)
                        .text_color(rgb(DANGER))
                        .child("Delete")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm = None;
                            let (ws, id) = (ws_c.clone(), id_c.clone());
                            this.state
                                .update(cx, |app, cx| app.remove_session(&ws, &id, cx));
                        })),
                )
                .child(
                    btn(format!("del-no-{id}"), TEXT)
                        .child("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm = None;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let (ws_r, id_r, id_d) = (ws.clone(), id.clone(), id.clone());
        div()
            .flex()
            .items_center()
            .child(
                btn(format!("ren-{id}"), TEXT)
                    .child(icon(I_EDIT, 12., MUTED))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let (ws, id, t) = (ws_r.clone(), id_r.clone(), t.clone());
                        this.state
                            .update(cx, |app, cx| app.begin_rename(&ws, &id, &t, cx));
                    })),
            )
            .child(
                btn(format!("del-{id}"), DANGER)
                    .child(icon(I_CLOSE, 11., MUTED))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.confirm = Some(format!("del:{id_d}"));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// Progressive loading row under a truncated session list: the first
    /// click extends the visible window, the second shows everything.
    pub(crate) fn load_more_row(
        &self,
        ws_key: &str,
        step: u8,
        hidden: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::sessions::items::{load_more_label, next_limit_step};
        let key = ws_key.to_string();
        div()
            .id(SharedString::from(format!("load-more-{ws_key}")))
            .pl(px(34.))
            .pr(px(12.))
            .h(px(24.))
            .flex()
            .items_center()
            .text_size(px(ui_size(11.)))
            .text_color(rgb(MUTED))
            .hover(|s| s.text_color(rgb(TEXT)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                let next = next_limit_step(step);
                this.session_limit_step.insert(key.clone(), next);
                cx.notify();
            }))
            .child(load_more_label(step, hidden))
            .into_any_element()
    }
}
