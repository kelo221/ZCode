//! Right-click menu on a sidebar project row (desktop parity:
//! project-sidebar/ProjectNodeContent.tsx context menu, plus the task-row
//! file-manager / copy-path actions applied to the project folder).

use crate::app::root::RootView;
use crate::shared::os::file_launcher;
use ely_gpui_component::menus::{Menu, MenuItem};
use gpui::{App, ClipboardItem, Context, Window};
use std::path::{Path, PathBuf};

fn file_manager_label() -> &'static str {
    if cfg!(windows) {
        crate::shared::i18n::t("appHeader.openInFileExplorer")
    } else if cfg!(target_os = "macos") {
        crate::shared::i18n::t("appHeader.openInFinder")
    } else {
        crate::shared::i18n::t("appHeader.openInFileManager")
    }
}

impl RootView {
    /// "New task" for a project: the row's "+" button and its menu entry.
    pub(crate) fn start_project_task(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_settings(window, cx);
        self.show_chat_page();
        self.state
            .update(cx, |s, cx| s.set_active_workspace(key, cx));
    }

    /// Drop the project from the sidebar and forget its folder state.
    fn remove_project(&mut self, key: &str, cx: &mut Context<Self>) {
        let removed = self.state.update(cx, |s, cx| s.remove_workspace(key, cx));
        if removed {
            self.folders.collapsed.remove(key);
            self.folders.opened.remove(key);
            self.session_limit_step.remove(key);
            self.state.update(cx, |s, _| s.sidebar_cache.forget(key));
        }
        cx.notify();
    }

    fn launch_error(&mut self, what: &str, path: &Path, error: std::io::Error, cx: &mut App) {
        self.state.update(cx, |s, cx| {
            s.push_error(format!("{what} {}: {error}", path.display()));
            cx.notify();
        });
    }

    pub(crate) fn project_menu(&self, key: &str, path: PathBuf, cx: &mut Context<Self>) -> Menu {
        let view = cx.weak_entity();
        let (v_new, k_new) = (view.clone(), key.to_string());
        let (v_files, p_files) = (view.clone(), path.clone());
        let (v_editor, p_editor) = (view.clone(), path.clone());
        let (v_remove, k_remove) = (view, key.to_string());
        let copy_text = path.to_string_lossy().into_owned();
        Menu::new()
            .item(
                MenuItem::new(crate::shared::i18n::t("quickPick.command.newTask")).on_click(
                    move |window, cx| {
                        let _ = v_new
                            .update(cx, |this, cx| this.start_project_task(&k_new, window, cx));
                    },
                ),
            )
            .separator()
            .item(MenuItem::new(file_manager_label()).on_click(move |_, cx| {
                let (view, path) = (v_files.clone(), p_files.clone());
                file_launcher::open_with_system_default(p_files.clone(), cx, move |e, cx| {
                    let _ = view.update(cx, |this, cx| this.launch_error("open", &path, e, cx));
                });
            }))
            .item(
                MenuItem::new(crate::shared::i18n::label(
                    "Open in editor",
                    "在编辑器中打开",
                ))
                .on_click(move |_, cx| {
                    let (view, path) = (v_editor.clone(), p_editor.clone());
                    file_launcher::open_in_editor(p_editor.clone(), cx, move |e, cx| {
                        let _ = view.update(cx, |this, cx| this.launch_error("open", &path, e, cx));
                    });
                }),
            )
            .item(
                MenuItem::new(crate::shared::i18n::t("appHeader.copyPath")).on_click(
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()))
                    },
                ),
            )
            .separator()
            .item(
                MenuItem::new(crate::shared::i18n::label(
                    "Remove from sidebar",
                    "从侧边栏移除",
                ))
                .on_click(move |_, cx| {
                    let _ = v_remove.update(cx, |this, cx| this.remove_project(&k_remove, cx));
                }),
            )
    }
}
