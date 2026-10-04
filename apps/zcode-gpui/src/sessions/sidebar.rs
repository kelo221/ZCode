//! Sidebar: "New task", "Search", "Tasks" (chats without a project), and
//! "Projects" with their sessions underneath, laid out like the desktop sidebar.

use crate::backend::workspace::WorkspacePurpose;
use crate::conversation::model::{format_preview, phase_is_active};
pub use crate::sessions::items::filter_session_indices;
use crate::sessions::items::{ROW_GROUP, format_relative_time, nav_row};
use crate::shared::theme::{
    ACCENT, BG, DANGER, HOVER, I_ADD, I_CLOSE, I_EDIT, I_FOLDER, I_SEARCH, MUTED, TEXT, icon,
};
use gpui::{AnyElement, ClickEvent, Context, CursorStyle, SharedString, div, prelude::*, px, rgb};

impl crate::app::root::RootView {
    fn session_row(
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
                .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
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
                                .text_size(px(12.))
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
                .text_size(px(12.))
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

    fn project_row(
        &self,
        key: &str,
        display: &str,
        status: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Idle (not yet spawned) is the normal state with lazy agents, so it
        // stays quiet; only real failures are shown.
        let failed =
            status.starts_with("error") || status.contains("failed") || status == "no backend";
        let key_c = key.to_string();
        let key_new = key.to_string();
        // Desktop parity (workspace-grouped-tasks/group-item.tsx): every
        // project header carries a "New task" button that starts a draft there.
        let new_task = div()
            .id(SharedString::from(format!("ws-new-{key}")))
            .px_1()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(HOVER)))
            .child(icon(I_ADD, 12., MUTED))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                this.state
                    .update(cx, |s, cx| s.set_active_workspace(&key_new, cx));
            }));
        nav_row(SharedString::from(format!("ws-{key}")), false)
            .mt_1()
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                this.state
                    .update(cx, |s, cx| s.set_active_workspace(&key_c, cx));
            }))
            .child(icon(I_FOLDER, 14., if active { TEXT } else { MUTED }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(if active { TEXT } else { 0xc8c8c8 }))
                    .child(display.to_string()),
            )
            .when(failed, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(DANGER))
                        .child(status.to_string()),
                )
            })
            .child(new_task)
            .into_any_element()
    }

    /// Snapshot of all workspaces + their session rows, grouped by Tasks & Projects.
    pub(crate) fn sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (active_workspace, tasks, projects) = {
            let state = self.state.read(cx);
            let active = state.active_ws_key();
            let search = self.session_search.to_lowercase();
            let child_sids = state.all_child_session_ids();
            let mut tasks: Vec<(String, Vec<usize>)> = Vec::new();
            let mut projects: Vec<(String, String, String, Vec<usize>)> = Vec::new();

            for w in &state.workspaces {
                let indices = filter_session_indices(&w.sessions, &child_sids, &search);
                match w.purpose {
                    WorkspacePurpose::Conversation => {
                        tasks.push((w.key.clone(), indices));
                    }
                    WorkspacePurpose::Project => {
                        projects.push((
                            w.key.clone(),
                            w.display.clone(),
                            w.status.clone(),
                            indices,
                        ));
                    }
                }
            }
            (active, tasks, projects)
        };

        let mut tasks_list: Vec<AnyElement> = Vec::new();
        let total_task_sessions: usize = tasks.iter().map(|(_, idxs)| idxs.len()).sum();
        if total_task_sessions == 0 {
            tasks_list.push(
                div()
                    .px_4()
                    .py_1p5()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("No tasks yet")
                    .into_any_element(),
            );
        } else {
            for (key, indices) in &tasks {
                tasks_list.extend(indices.iter().filter_map(|i| self.session_row(key, *i, cx)));
            }
        }

        let mut projects_list: Vec<AnyElement> = Vec::new();
        for (key, display, status, indices) in &projects {
            let active = active_workspace.as_deref() == Some(key.as_str());
            projects_list.push(self.project_row(key, display, status, active, cx));
            projects_list.extend(indices.iter().filter_map(|i| self.session_row(key, *i, cx)));
        }

        let footer: SharedString = {
            let state = self.state.read(cx);
            state
                .active_ws_key()
                .and_then(|k| state.ws(&k))
                .map(|w| format!("{} · {}", w.conn_desc, w.status))
                .unwrap_or_else(|| "no workspace".into())
                .into()
        };

        let tasks_header = div()
            .px_4()
            .pt_3()
            .pb_1()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("Tasks"),
            )
            .child(
                div()
                    .id("new-conversation-task")
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.bg(rgb(HOVER)))
                    .rounded_sm()
                    .p_1()
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.update(cx, |s, cx| s.new_conversation_chat(cx));
                    }))
                    .child(icon(I_ADD, 12., MUTED)),
            );

        div()
            .w(px(264.))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .pt_3()
            .child(
                nav_row("new-chat".into(), false)
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.state.update(cx, |s, cx| s.new_conversation_chat(cx));
                    }))
                    .child(icon(I_ADD, 13., TEXT))
                    .child(div().text_color(rgb(TEXT)).child("New task")),
            )
            .child(
                nav_row("search-cmd".into(), false)
                    .on_click(cx.listener(|this, _: &ClickEvent, _window, cx| {
                        this.quickpick_open = true;
                        this.quickpick_query.clear();
                        this.quickpick_selected = 0;
                        cx.notify();
                    }))
                    .child(icon(I_SEARCH, 13., MUTED))
                    .child(div().flex_1().text_color(rgb(MUTED)).child("Search"))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child("Ctrl+K"),
                    ),
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
                    .child(tasks_header)
                    .children(tasks_list)
                    .child(
                        div()
                            .px_4()
                            .pt_4()
                            .pb_1()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child("Projects"),
                    )
                    .children(projects_list),
            )
            .child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(footer),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod tests;
