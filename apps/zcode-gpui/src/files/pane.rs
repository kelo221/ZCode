//! Files pane: lazy workspace file tree (direct fs, local workspaces only)
//! with text / markdown preview (PARITY.md M3 "file tree + workspace file
//! preview").

use crate::app::root::RootView;
pub use crate::files::preview::Preview;
use crate::files::preview::{pane_note, preview_element, read_preview};
use crate::shared::theme::{BORDER, CARD, HOVER, MUTED, TEXT, TOOL};
use crate::shared::theme_colors::color as rgb;
use crate::shared::{i18n::label, theme::ui_size};
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FilesNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// `None` until the directory is expanded once (lazy loading).
    pub children: Option<Vec<FilesNode>>,
}

#[derive(Default)]
pub struct FilesState {
    /// Top-level entries (children load on expand).
    pub roots: Vec<FilesNode>,
    pub expanded: std::collections::HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub preview: Option<Preview>,
    pub loading: bool,
    /// Workspace the tree was loaded from; a different active workspace
    /// resets the pane.
    pub root: Option<PathBuf>,
    pub(crate) workspace: Option<String>,
    pub(crate) reset_generation: u64,
    pub(crate) generation: u64,
}

impl RootView {
    pub(crate) fn ensure_files_loaded(&mut self, cx: &mut Context<Self>) {
        let active = self.active_workspace_path(cx);
        let workspace = self.state.read(cx).active_ws_key();
        // 两个 identity 可以共享 path；目录归属不能只比较文件系统根。
        if active == self.files.root && workspace == self.files.workspace {
            return;
        }
        let generation = self
            .files
            .generation
            .checked_add(1)
            .expect("file preview generation exhausted");
        let reset_generation = self
            .files
            .reset_generation
            .checked_add(1)
            .expect("file workspace generation exhausted");
        self.files = FilesState {
            root: active,
            workspace,
            generation,
            reset_generation,
            ..FilesState::default()
        };
        self.load_dir(None, cx);
    }

    /// Load a directory's children in the background (`None` = workspace root).
    fn load_dir(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(root) = self.files.root.clone() else {
            return;
        };
        let Some(workspace) = self.files.workspace.clone() else {
            return;
        };
        let generation = self.files.reset_generation;
        if self.state.read(cx).active_ws_key().as_deref() != Some(workspace.as_str())
            || self.active_workspace_path(cx).as_ref() != Some(&root)
            || dir.as_ref().is_some_and(|path| !path.starts_with(&root))
        {
            return;
        }
        self.files.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let moved_dir = dir.clone();
            let list_root = root.clone();
            let entries = cx
                .background_spawn(async move { list_dir(&list_root, moved_dir.as_deref()) })
                .await;
            this.update(cx, |v, cx| {
                v.settle_file_listing(&workspace, &root, generation, dir, entries, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn settle_file_listing(
        &mut self,
        workspace: &str,
        root: &Path,
        generation: u64,
        dir: Option<PathBuf>,
        entries: Vec<FilesNode>,
        cx: &mut Context<Self>,
    ) {
        // 工作区 A→B→A 会复用根路径；旧 listing 不能覆盖新目录或结束其 loading。
        if self.files.workspace.as_deref() != Some(workspace)
            || self.files.root.as_deref() != Some(root)
            || self.files.reset_generation != generation
            || self.state.read(cx).active_ws_key().as_deref() != Some(workspace)
            || self.active_workspace_path(cx).as_deref() != Some(root)
        {
            return;
        }
        self.files.loading = false;
        match dir {
            None => self.files.roots = entries,
            Some(d) => {
                insert_children(&mut self.files.roots, &d, entries);
                self.files.expanded.insert(d);
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.files.expanded.remove(&path) {
            cx.notify();
            return;
        }
        self.load_dir(Some(path), cx);
    }

    pub(crate) fn select_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(root) = self.files.root.clone() else {
            return;
        };
        let Some(workspace) = self.state.read(cx).active_ws_key() else {
            return;
        };
        if self.files.workspace.as_deref() != Some(workspace.as_str())
            || self.active_workspace_path(cx).as_ref() != Some(&root)
            || !path.starts_with(&root)
        {
            return;
        }
        self.files.generation = self
            .files
            .generation
            .checked_add(1)
            .expect("file preview generation exhausted");
        let generation = self.files.generation;
        self.files.selected = Some(path.clone());
        self.files.preview = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let preview = cx
                .background_spawn(async move { read_preview(&read_path) })
                .await;
            this.update(cx, |v, cx| {
                v.settle_file_preview(&workspace, &root, &path, generation, preview, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn settle_file_preview(
        &mut self,
        workspace: &str,
        root: &Path,
        path: &Path,
        generation: u64,
        preview: Preview,
        cx: &mut Context<Self>,
    ) {
        // 同一路径的 A→B→A 与工作区重入仍是新请求；只按 path 会采用旧图片。
        if self.files.selected.as_deref() != Some(path)
            || self.files.workspace.as_deref() != Some(workspace)
            || self.files.root.as_deref() != Some(root)
            || self.files.generation != generation
            || self.state.read(cx).active_ws_key().as_deref() != Some(workspace)
            || self.active_workspace_path(cx).as_deref() != Some(root)
        {
            return;
        }
        self.files.preview = Some(preview);
        cx.notify();
    }

    /// Flattened visible tree (depth, node) respecting the expanded set.
    fn visible_rows(&self) -> Vec<(usize, &FilesNode)> {
        fn walk<'a>(
            nodes: &'a [FilesNode],
            depth: usize,
            expanded: &std::collections::HashSet<PathBuf>,
            out: &mut Vec<(usize, &'a FilesNode)>,
        ) {
            for n in nodes {
                out.push((depth, n));
                if n.is_dir
                    && expanded.contains(&n.path)
                    && let Some(children) = &n.children
                {
                    walk(children, depth + 1, expanded, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.files.roots, 0, &self.files.expanded, &mut out);
        out
    }

    pub(crate) fn files_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.visible_rows();
        div()
            .flex()
            .flex_row()
            .size_full()
            .min_h_0()
            .text_color(rgb(TEXT))
            // Tree column.
            .child(
                div()
                    .id("files-tree")
                    .w(px(190.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_y_scroll()
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .when(self.files.roots.is_empty() && self.files.loading, |el| {
                        el.child(pane_note(label("Loading…", "加载中…")))
                    })
                    .when(self.files.roots.is_empty() && !self.files.loading, |el| {
                        el.child(pane_note(label(
                            "No files in this workspace",
                            "此工作区没有文件",
                        )))
                    })
                    .children(
                        rows.into_iter()
                            .map(|(depth, node)| self.files_row(depth, node, cx)),
                    ),
            )
            // Preview column.
            .child(
                div()
                    .id("files-preview")
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(match (&self.files.selected, &self.files.preview) {
                        (None, _) => pane_note(label("Select a file to preview", "选择文件以预览")),
                        (Some(_), None) => pane_note(label("Loading…", "加载中…")),
                        (Some(p), Some(pv)) => preview_element(p, pv),
                    }),
            )
            .into_any_element()
    }

    fn files_row(&self, depth: usize, node: &FilesNode, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.files.selected.as_ref() == Some(&node.path);
        let expanded = self.files.expanded.contains(&node.path);
        let name = node.name.clone();
        let click_path = node.path.clone();
        let is_dir = node.is_dir;
        let root = self.files.root.clone();
        let workspace = self.state.read(cx).active_ws_key();
        let generation = self.files.generation;
        let row = div()
            .id(SharedString::from(format!("f-{}", node.path.display())))
            .flex()
            .items_center()
            .gap_1()
            .pl(px(6. + depth as f32 * 12.))
            .pr_1()
            .py_0p5()
            .rounded_sm()
            .mr_1()
            .cursor(CursorStyle::PointingHand)
            .when(selected, |el| el.bg(rgb(CARD)))
            .when(!selected, |el| el.hover(|h| h.bg(rgb(HOVER))))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if this.files.root != root
                    || this.files.generation != generation
                    || this.state.read(cx).active_ws_key() != workspace
                {
                    return;
                }
                if is_dir {
                    this.toggle_dir(click_path.clone(), cx);
                } else {
                    this.select_file(click_path.clone(), cx);
                }
            }))
            .child(
                div()
                    .w(px(10.))
                    .text_size(px(ui_size(9.)))
                    .text_color(rgb(MUTED))
                    .child(if is_dir {
                        if expanded { "▾" } else { "▸" }.to_string()
                    } else {
                        String::new()
                    }),
            )
            .child(
                div()
                    .text_size(px(ui_size(11.5)))
                    .truncate()
                    .text_color(rgb(if node.is_dir { TOOL } else { TEXT }))
                    .child(name),
            );
        #[cfg(test)]
        let row = crate::app::test_support::track_children(
            div().child(row),
            vec![format!("file-row-{}", node.name)],
        );
        row.into_any_element()
    }
}

/// Insert freshly listed children under `dir` anywhere in the tree.
fn insert_children(nodes: &mut [FilesNode], dir: &Path, entries: Vec<FilesNode>) {
    let mut entries = Some(entries);
    for n in nodes.iter_mut() {
        if n.path == dir {
            n.children = entries.take();
            return;
        }
        if let (Some(children), Some(list)) = (n.children.as_mut(), entries.as_ref()) {
            insert_children(children, dir, list.clone());
        }
        if entries.is_none() {
            return;
        }
    }
}

/// List one directory: dirs first, then files, both alphabetically.
fn list_dir(root: &Path, rel: Option<&Path>) -> Vec<FilesNode> {
    let dir = match rel {
        Some(r) => root.join(r),
        None => root.to_path_buf(),
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in read.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let node = FilesNode {
            path: entry.path(),
            is_dir: ft.is_dir(),
            name,
            children: None,
        };
        if ft.is_dir() {
            dirs.push(node);
        } else {
            files.push(node);
        }
    }
    dirs.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    dirs.extend(files);
    dirs
}
