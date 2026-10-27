//! Sidebar: "New task", "Search", "Tasks" (chats without a project), and
//! "Projects" with their sessions underneath, laid out like the desktop
//! sidebar. Session lists load progressively (3 latest, then two "Load more"
//! clicks to everything — see sessions/items.rs) and project folders toggle
//! open/closed on the header (desktop parity:
//! workspace-grouped-tasks/group-item.tsx handleHeaderClick); an open folder
//! stays open when another project becomes active. The desktop
//! virtualizes its list, so it never needed these caps; our plain rows do.

use crate::backend::workspace::WorkspacePurpose;
pub use crate::sessions::items::filter_session_indices;
use crate::sessions::items::{nav_row, sort_by_latest_activity};
use crate::sessions::section_header::section_header;
use crate::shared::theme::ui_size;
use crate::shared::theme::{
    BORDER, DANGER, FRAME, HOVER, I_ADD, I_CHEVRON_DOWN, I_CHEVRON_RIGHT, I_FOLDER, I_SEARCH,
    MUTED, TEXT, icon,
};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::menus::ContextMenu;
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*, px};

/// One workspace's rendered rows: the visible session rows plus, when the
/// list was truncated, a "Load more" row for the hidden remainder.
struct VisibleRows {
    rows: Vec<AnyElement>,
    hidden: usize,
}

/// One project group in the snapshot: key, folder path, display name, agent
/// status, the filtered session indices, and the active thread's position
/// within them.
type ProjectGroup = (
    String,
    std::path::PathBuf,
    String,
    String,
    Vec<usize>,
    Option<usize>,
);

/// Header-row facts `project_row` renders.
struct ProjectRow<'a> {
    key: &'a str,
    path: &'a std::path::Path,
    display: &'a str,
    status: &'a str,
    active: bool,
    expanded: bool,
}

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

    fn project_row(&self, row: ProjectRow<'_>, cx: &mut Context<Self>) -> AnyElement {
        let ProjectRow {
            key,
            path,
            display,
            status,
            active,
            expanded,
        } = row;
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
                cx.stop_propagation();
                this.start_project_task(&key_new, window, cx);
            }));
        let key_toggle = key.to_string();
        let expanded_at_render = expanded;
        let row = nav_row(SharedString::from(format!("ws-{key}")), false)
            .h(px(32.))
            .pl(px(6.))
            .mt_1()
            // Folder toggle (desktop handleHeaderClick): an open folder
            // closes; a closed one opens, and activates when it isn't the
            // active project so its agent loads the sessions. The "+" still
            // just starts a draft.
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.close_settings(window, cx);
                cx.stop_propagation();
                this.folders.toggle(&key_toggle, expanded_at_render);
                if !expanded_at_render && !active {
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
                    .text_color(rgb(TEXT))
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
            .child(new_task);
        let menu = self.project_menu(key, path.to_path_buf(), cx);
        let row =
            ContextMenu::new(SharedString::from(format!("project-ctx-{key}")), menu).child(row);
        #[cfg(test)]
        let row = crate::app::test_support::track_children(
            div().child(row),
            vec![format!("project-row-{display}")],
        );
        row.into_any_element()
    }

    /// Snapshot of all workspaces + their session rows, grouped by Tasks & Projects.
    pub(crate) fn sidebar(&mut self, full_width: bool, cx: &mut Context<Self>) -> AnyElement {
        let (active_workspace, tasks, projects) = {
            let state = self.state.read(cx);
            let active = state.active_ws_key();
            let child_sids = state.all_child_session_ids();
            let active_sid = state.active.clone();
            // (ws key, filtered indices, position of the active thread within them)
            let mut tasks: Vec<(String, Vec<usize>, Option<usize>)> = Vec::new();
            let mut projects: Vec<(Option<i64>, ProjectGroup)> = Vec::new();

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
                        let latest = state.project_sort_activity(w, &indices);
                        projects.push((
                            latest,
                            (
                                w.key.clone(),
                                w.path.clone(),
                                w.display.clone(),
                                w.status.clone(),
                                indices,
                                active_index,
                            ),
                        ));
                    }
                }
            }
            // Render-time order only: `state.workspaces` keeps discovery order
            // (startup project lookup relies on it).
            sort_by_latest_activity(&mut projects, |(latest, _)| *latest);
            let projects: Vec<ProjectGroup> = projects.into_iter().map(|(_, p)| p).collect();
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
                    .child(crate::shared::i18n::t("taskList.noTasks"))
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
        for (key, path, display, status, indices, active_index) in &projects {
            let active = active_workspace.as_deref() == Some(key.as_str());
            let expanded = self.folders.is_open(key, active);
            let row = ProjectRow {
                key,
                path,
                display,
                status,
                active,
                expanded,
            };
            projects_list.push(self.project_row(row, cx));
            if expanded {
                let VisibleRows { mut rows, hidden } =
                    self.visible_session_rows(key, indices, *active_index, cx);
                if hidden > 0 {
                    let step = self.session_limit_step.get(key).copied().unwrap_or(0);
                    rows.push(self.load_more_row(key, step, hidden, cx));
                }
                // Desktop group content: indented under the header with a guide line.
                if rows.is_empty() {
                    continue;
                }
                projects_list.push(
                    div()
                        .ml(px(24.))
                        .py(px(1.))
                        .border_l_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .flex_col()
                        .children(rows)
                        .into_any_element(),
                );
            }
        }

        let tasks_header = section_header(
            "new-conversation-task",
            crate::shared::i18n::label("Tasks", "任务").into(),
            cx.listener(|this, _: &ClickEvent, window, cx| {
                this.close_settings(window, cx);
                this.show_chat_page();
                this.state.update(cx, |s, cx| s.new_conversation_chat(cx));
            }),
        );
        // Electron WorkspaceSidebar: "+" on the projects header → Open folder.
        let projects_header = section_header(
            "add-project",
            crate::shared::i18n::label("Projects", "项目").into(),
            cx.listener(|this, _: &ClickEvent, window, cx| {
                this.close_settings(window, cx);
                this.open_project_dialog(cx);
            }),
        );

        let sidebar = div()
            .map(|el| {
                if full_width {
                    el.w_full()
                } else {
                    el.w(px(264.))
                }
            })
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(FRAME))
            .pt_3()
            .child(
                nav_row("new-chat".into(), false)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.close_settings(window, cx);
                        this.show_chat_page();
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
            .child(self.sidebar_footer(cx));
        #[cfg(test)]
        let sidebar = crate::app::test_support::track_children(
            div().child(sidebar),
            vec!["workspace-sidebar".into()],
        );
        sidebar.into_any_element()
    }
}

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sidebar_render_tests.rs"]
mod render_tests;
