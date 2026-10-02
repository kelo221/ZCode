//! Sidebar: all projects (from ~/.zcode/v2/setting.json + the CLI workspace),
//! sessions grouped under each project header.

use crate::model::format_preview;
use crate::theme::{ACCENT, BG, BORDER, CARD, DANGER, MUTED, PANEL, TEXT};
use gpui::{
    AnyElement, ClickEvent, Context, CursorStyle, FontWeight, IntoElement, ParentElement,
    SharedString, Styled, div, prelude::*, px, rgb,
};

fn format_relative_time(last_activity_ms: Option<i64>) -> String {
    let Some(ts) = last_activity_ms else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let diff_secs = (now - ts).max(0) / 1000;
    if diff_secs < 60 {
        "just now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h", diff_secs / 3600)
    } else {
        format!("{}d", diff_secs / 86400)
    }
}

impl crate::ui::RootView {
    fn session_row(
        &self,
        ws_key: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (sid, full_title, title, preview, phase, is_active, rel_time) = {
            let state = self.state.read(cx);
            let ws = state.ws(ws_key)?;
            let session = ws.sessions.get(index)?;
            (
                session.session_id.clone(),
                session.title.clone(),
                format_preview(&session.title, 36),
                format_preview(&session.preview, 54),
                session.phase.clone(),
                state.active.as_deref() == Some(session.session_id.as_str()),
                format_relative_time(session.last_activity_at),
            )
        };
        let title: SharedString = title.into();
        let preview: SharedString = preview.into();
        let ws_key = ws_key.to_string();
        let confirming = self.confirm.as_deref() == Some(format!("del:{sid}").as_str());
        let controls = self.session_controls(&ws_key, &sid, &full_title, confirming, cx);
        Some(
            div()
                .id(SharedString::from(sid.clone()))
                .mx_2()
                .pl_4()
                .pr_2()
                .py_1p5()
                .rounded_md()
                .cursor(CursorStyle::PointingHand)
                .when(is_active, |el| el.bg(rgb(CARD)))
                .hover(|s| s.bg(rgb(CARD)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                    this.state
                        .update(cx, |s, cx| s.select_session(&ws_key, &sid, cx));
                }))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(TEXT))
                                .truncate()
                                .child(title),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .when(!rel_time.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(MUTED))
                                            .child(rel_time),
                                    )
                                })
                                .child(crate::theme::phase_badge(&phase))
                                .child(controls),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .truncate()
                        .child(preview),
                )
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
        let btn = |id: String, label: &'static str, hover: u32| {
            div()
                .id(SharedString::from(id))
                .px_1()
                .text_size(px(10.))
                .text_color(rgb(MUTED))
                .hover(move |s| s.text_color(rgb(hover)))
                .cursor_pointer()
                .child(label)
        };
        let (ws, id, t) = (ws_key.to_string(), sid.to_string(), title.to_string());
        if confirming {
            let (ws_c, id_c) = (ws.clone(), id.clone());
            return div()
                .flex()
                .items_center()
                .child(
                    btn(format!("del-ok-{id}"), "Delete", DANGER)
                        .text_color(rgb(DANGER))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm = None;
                            let (ws, id) = (ws_c.clone(), id_c.clone());
                            this.state
                                .update(cx, |app, cx| app.remove_session(&ws, &id, cx));
                        })),
                )
                .child(
                    btn(format!("del-no-{id}"), "Cancel", TEXT).on_click(cx.listener(
                        |this, _, _, cx| {
                            cx.stop_propagation();
                            this.confirm = None;
                            cx.notify();
                        },
                    )),
                )
                .into_any_element();
        }
        let (ws_r, id_r, id_d) = (ws.clone(), id.clone(), id.clone());
        div()
            .flex()
            .items_center()
            .child(btn(format!("ren-{id}"), "✎", TEXT).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    let (ws, id, t) = (ws_r.clone(), id_r.clone(), t.clone());
                    this.state
                        .update(cx, |app, cx| app.begin_rename(&ws, &id, &t, cx));
                },
            )))
            .child(btn(format!("del-{id}"), "✕", DANGER).on_click(cx.listener(
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.confirm = Some(format!("del:{id_d}"));
                    cx.notify();
                },
            )))
            .into_any_element()
    }

    /// Snapshot of all workspaces + their session rows, grouped per project.
    pub(crate) fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active_workspace = self.state.read(cx).active_ws_key();
        let search = self.session_search.to_lowercase();
        let sidebar: Vec<(String, String, String, bool, Vec<usize>)> = self
            .state
            .read(cx)
            .workspaces
            .iter()
            .map(|w| {
                let indices = (0..w.sessions.len())
                    .filter(|&i| {
                        if search.is_empty() {
                            true
                        } else {
                            let s = &w.sessions[i];
                            s.title.to_lowercase().contains(&search)
                                || s.preview.to_lowercase().contains(&search)
                        }
                    })
                    .collect();
                (
                    w.key.clone(),
                    w.display.clone(),
                    w.status.clone(),
                    w.inbound.is_some(),
                    indices,
                )
            })
            .collect();
        let ws_count = sidebar.len();

        div()
            .w(px(264.))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .px_3()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("ZCode"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(format!("{ws_count} projects")),
                    ),
            )
            .child(
                div()
                    .id("new-chat")
                    .mx_2()
                    .mb_2()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(12.5))
                    .text_color(rgb(ACCENT))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.bg(rgb(CARD)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.update(cx, |s, cx| s.new_chat(cx));
                    }))
                    .child("+ New chat"),
            )
            .child(
                div()
                    .id("session-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .pb_2()
                    .children(
                        sidebar
                            .iter()
                            .flat_map(|(key, display, status, connected, indices)| {
                                let key = key.clone();
                                let active_ws_here =
                                    active_workspace.as_deref() == Some(key.as_str());
                                let header = div()
                                    .id(SharedString::from(format!("ws-{key}")))
                                    .px_3()
                                    .py_1p5()
                                    .mt_1()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .cursor(CursorStyle::PointingHand)
                                    .when(active_ws_here, |el| el.bg(rgb(PANEL)))
                                    .hover(|s| s.bg(rgb(PANEL)))
                                    .on_click(cx.listener({
                                        let key = key.clone();
                                        move |this, _: &ClickEvent, _window, cx| {
                                            this.state.update(cx, |s, cx| {
                                                s.set_active_workspace(&key, cx)
                                            });
                                        }
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(if active_ws_here {
                                                ACCENT
                                            } else {
                                                MUTED
                                            }))
                                            .child(display.clone()),
                                    )
                                    .children((!connected).then(|| {
                                        div()
                                            .text_size(px(9.5))
                                            .text_color(rgb(DANGER))
                                            .child(status.clone())
                                    }))
                                    .into_any_element();
                                let rows = indices
                                    .iter()
                                    .filter_map(|i| self.session_row(&key, *i, cx))
                                    .collect::<Vec<AnyElement>>();
                                let mut items = Vec::with_capacity(rows.len() + 1);
                                items.push(header);
                                items.extend(rows);
                                items
                            })
                            .collect::<Vec<AnyElement>>(),
                    ),
            )
            .child({
                let state = self.state.read(cx);
                let status: SharedString = state
                    .active_ws_key()
                    .and_then(|k| state.ws(&k))
                    .map(|w| format!("{} · {}", w.conn_desc, w.status))
                    .unwrap_or_else(|| "no workspace".into())
                    .into();
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(10.5))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(status)
            })
            .into_any_element()
    }
}
