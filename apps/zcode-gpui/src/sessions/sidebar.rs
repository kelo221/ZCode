//! Sidebar: "New task", "Search", "Tasks" (chats without a project), and
//! "Projects" with their sessions underneath, laid out like the desktop
//! sidebar. Session lists load progressively (3 latest, then two "Load more"
//! clicks to everything — see sessions/items.rs) and project folders toggle
//! open/closed on the header (desktop parity:
//! workspace-grouped-tasks/group-item.tsx handleHeaderClick). The desktop
//! virtualizes its list, so it never needed these caps; our plain rows do.

use crate::backend::workspace::WorkspacePurpose;
pub use crate::sessions::items::filter_session_indices;
use crate::sessions::items::nav_row;
use crate::shared::theme::ui_size;
use crate::shared::theme::{
    BG, DANGER, HOVER, I_ADD, I_CHEVRON_DOWN, I_CHEVRON_RIGHT, I_FOLDER, I_SEARCH, MUTED, TEXT,
    icon,
};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, ClickEvent, Context, CursorStyle, SharedString, div, prelude::*, px};

/// One workspace's rendered rows: the visible session rows plus, when the
/// list was truncated, a "Load more" row for the hidden remainder.
struct VisibleRows {
    rows: Vec<AnyElement>,
    hidden: usize,
}

/// One project group in the snapshot: key, display name, agent status, the
/// filtered session indices, and the active thread's position within them.
type ProjectGroup = (String, String, String, Vec<usize>, Option<usize>);

impl crate::app::root::RootView {
    /// Visible rows for one workspace's filtered session list: the preview
    /// window (extended by "Load more" clicks), never hiding the active
    /// thread, plus a load-more row while rows remain hidden.
    fn visible_session_rows(
        &mut self,
        key: &str,
        indices: &[usize],
        active_index: Option<usize>,
        cx: &mut Context<Self>,
    ) -> VisibleRows {
        let step = self.session_limit_step.get(key).copied().unwrap_or(0);
        let (visible, pinned) =
            crate::sessions::items::visible_row_indices(step, indices.len(), active_index);
        // Cap-preserving pinned active row (review finding 12.1).
        let mut rows: Vec<AnyElement> = indices
            .iter()
            .take(visible)
            .filter_map(|i| self.session_row(key, *i, cx))
            .collect();
        if let Some(ai) = pinned
            && let Some(row) = self.session_row(key, indices[ai], cx)
        {
            rows.push(row);
        }
        VisibleRows {
            rows,
            hidden: indices.len() - visible,
        }
    }

    /// Native directory picker → `AppState::add_workspace` (Electron parity:
    /// Projects-header "+" → Open folder → `PlatformChannels.SelectDirectory`).
    /// The dialog runs on its own thread; the result lands through a oneshot,
    /// so the UI never blocks on the modal dialog.
    pub(crate) fn open_project_dialog(&mut self, cx: &mut Context<Self>) {
        let rx = crate::shared::os::folder_picker::pick_directory();
        cx.spawn(async move |this, cx| {
            let outcome = rx
                .await
                .unwrap_or_else(|_| Err("folder picker thread died".into()));
            match outcome {
                Ok(Some(path)) => {
                    let _ = this.update(cx, |v, cx| {
                        v.state.update(cx, |s, cx| s.add_workspace(&path, cx));
                    });
                }
                Ok(None) => {} // user cancelled
                Err(e) => {
                    let _ = this.update(cx, |v, cx| {
                        v.state
                            .update(cx, |s, _| s.push_error(format!("folder picker: {e}")));
                    });
                }
            }
        })
        .detach();
    }

    fn project_row(
        &self,
        key: &str,
        display: &str,
        status: &str,
        active: bool,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Idle (not yet spawned) is the normal state with lazy agents, so it
        // stays quiet; only real failures are shown.
        let failed =
            status.starts_with("error") || status.contains("failed") || status == "no backend";
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
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.close_settings(window, cx);
                cx.stop_propagation();
                this.state
                    .update(cx, |s, cx| s.set_active_workspace(&key_new, cx));
            }));
        let key_toggle = key.to_string();
        let is_active_at_render = active;
        nav_row(SharedString::from(format!("ws-{key}")), false)
            .mt_1()
            // Folder toggle (desktop handleHeaderClick): the active folder
            // collapses/expands on click; a collapsed folder opens (and
            // activates) on click. The "+" still just starts a draft.
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.close_settings(window, cx);
                cx.stop_propagation();
                if is_active_at_render {
                    if !this.ws_collapsed.remove(&key_toggle) {
                        this.ws_collapsed.insert(key_toggle.clone());
                    }
                } else {
                    this.ws_collapsed.remove(&key_toggle);
                    this.state
                        .update(cx, |s, cx| s.set_active_workspace(&key_toggle, cx));
                }
                cx.notify();
            }))
            .child(icon(
                if expanded {
                    I_CHEVRON_DOWN
                } else {
                    I_CHEVRON_RIGHT
                },
                12.,
                MUTED,
            ))
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
                        .text_size(px(ui_size(11.)))
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
            let child_sids = state.all_child_session_ids();
            let active_sid = state.active.clone();
            // (ws key, filtered indices, position of the active thread within them)
            let mut tasks: Vec<(String, Vec<usize>, Option<usize>)> = Vec::new();
            let mut projects: Vec<ProjectGroup> = Vec::new();

            for w in &state.workspaces {
                let indices = filter_session_indices(&w.sessions, &child_sids, "");
                let active_index = active_sid.as_ref().and_then(|sid| {
                    w.sessions
                        .iter()
                        .position(|s| &s.session_id == sid)
                        .and_then(|i| indices.iter().position(|&x| x == i))
                });
                match w.purpose {
                    WorkspacePurpose::Conversation => {
                        tasks.push((w.key.clone(), indices, active_index));
                    }
                    WorkspacePurpose::Project => {
                        projects.push((
                            w.key.clone(),
                            w.display.clone(),
                            w.status.clone(),
                            indices,
                            active_index,
                        ));
                    }
                }
            }
            (active, tasks, projects)
        };

        let mut tasks_list: Vec<AnyElement> = Vec::new();
        let total_task_sessions: usize = tasks.iter().map(|(_, idxs, _)| idxs.len()).sum();
        if total_task_sessions == 0 {
            tasks_list.push(
                div()
                    .px_4()
                    .py_1p5()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child("No tasks yet")
                    .into_any_element(),
            );
        } else {
            for (key, indices, active_index) in &tasks {
                let VisibleRows { rows, hidden } =
                    self.visible_session_rows(key, indices, *active_index, cx);
                tasks_list.extend(rows);
                if hidden > 0 {
                    let step = self.session_limit_step.get(key).copied().unwrap_or(0);
                    tasks_list.push(self.load_more_row(key, step, hidden, cx));
                }
            }
        }

        let mut projects_list: Vec<AnyElement> = Vec::new();
        for (key, display, status, indices, active_index) in &projects {
            let active = active_workspace.as_deref() == Some(key.as_str());
            // Folder rule: the active project is open unless the user closed
            // it; other projects stay closed until clicked (which opens them).
            let expanded = active && !self.ws_collapsed.contains(key.as_str());
            projects_list.push(self.project_row(key, display, status, active, expanded, cx));
            if expanded {
                let VisibleRows { rows, hidden } =
                    self.visible_session_rows(key, indices, *active_index, cx);
                projects_list.extend(rows);
                if hidden > 0 {
                    let step = self.session_limit_step.get(key).copied().unwrap_or(0);
                    projects_list.push(self.load_more_row(key, step, hidden, cx));
                }
            }
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
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(crate::shared::i18n::label("Tasks", "任务")),
            )
            .child(
                div()
                    .id("new-conversation-task")
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.bg(rgb(HOVER)))
                    .rounded_sm()
                    .p_1()
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_settings(window, cx);
                        this.state.update(cx, |s, cx| s.new_conversation_chat(cx));
                    }))
                    .child(icon(I_ADD, 12., MUTED)),
            );

        // Projects section header with the add-project button (Electron
        // WorkspaceSidebar: "+" on the projects header → Open folder).
        let projects_header = div()
            .px_4()
            .pt_4()
            .pb_1()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(crate::shared::i18n::label("Projects", "项目")),
            )
            .child(
                div()
                    .id("add-project")
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.bg(rgb(HOVER)))
                    .rounded_sm()
                    .p_1()
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_settings(window, cx);
                        this.open_project_dialog(cx);
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
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_settings(window, cx);
                        this.state.update(cx, |s, cx| s.new_conversation_chat(cx));
                    }))
                    .child(icon(I_ADD, 13., TEXT))
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .child(crate::shared::i18n::t("quickPick.command.newTask")),
                    ),
            )
            .child(
                nav_row("search-cmd".into(), false)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_command_center(window, cx);
                    }))
                    .child(icon(I_SEARCH, 13., MUTED))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(MUTED))
                            .child(crate::shared::i18n::label("Search", "搜索")),
                    )
                    .child(
                        div()
                            .text_size(px(ui_size(11.)))
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
                    .child(projects_header)
                    .children(projects_list),
            )
            .child(self.settings_footer(cx))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .text_size(px(ui_size(11.)))
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
