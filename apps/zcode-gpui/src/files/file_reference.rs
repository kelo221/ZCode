//! Opening a file reference clicked in the transcript (PARITY.md "Transcript
//! file references"): workspace files preview in the Files pane, `:line`
//! references open in the editor, other files use the system default app.

use crate::app::root::RootView;
use crate::shared::markdown::file_ref::{is_bare_name, parse_target, resolve_path};
use gpui::{AppContext, Context};
use std::path::{Path, PathBuf};

/// Existing file for `path`; a bare name missing at the workspace root is
/// looked up by name and used only when exactly one file matches. Blocking.
pub(crate) fn locate(root: Option<&Path>, home: Option<&Path>, path: &str) -> Option<PathBuf> {
    if let Some(candidate) = resolve_path(root, home, path)
        && candidate.is_file()
    {
        return Some(candidate);
    }
    let root = root.filter(|_| is_bare_name(path))?;
    match crate::composer::file_index::files_named(root, path).as_slice() {
        [only] => Some(root.join(only)),
        _ => None,
    }
}

impl RootView {
    pub(crate) fn open_file_reference(&mut self, raw: &str, cx: &mut Context<Self>) {
        let target = parse_target(raw);
        let root = self.active_workspace_path(cx);
        let home = crate::shared::settings::resolve_user_home_dir();
        let workspace = self.state.read(cx).active_ws_key();
        let lookup_root = root.clone();
        let path = target.path.clone();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { locate(lookup_root.as_deref(), Some(&home), &path) })
                .await;
            this.update(cx, |view, cx| {
                // 查找期间切换了工作区：结果属于旧工作区，不能在新工作区的 Files 面板里打开。
                if view.state.read(cx).active_ws_key() != workspace {
                    return;
                }
                let Some(found) = found else {
                    view.state.update(cx, |s, cx| {
                        s.push_error(format!("File not found: {}", target.path));
                        cx.notify();
                    });
                    return;
                };
                view.open_located_file(found, target.line, cx);
            })
            .ok();
        })
        .detach();
    }

    // 用户要求点击文件链接用系统默认程序打开，而不是右侧 Files 预览面板。
    fn open_located_file(&mut self, path: PathBuf, line: Option<u32>, cx: &mut Context<Self>) {
        let state = self.state.clone();
        let report = move |error: std::io::Error, cx: &mut gpui::App| {
            state.update(cx, |s, cx| {
                s.push_error(format!("Could not open file: {error}"));
                cx.notify();
            });
        };
        use crate::shared::os::file_launcher::{open_in_editor_at, open_with_system_default};
        match line {
            Some(line) => open_in_editor_at(path, line, cx, report),
            None => open_with_system_default(path, cx, report),
        }
    }
}

#[cfg(test)]
#[path = "file_reference_tests.rs"]
mod tests;
