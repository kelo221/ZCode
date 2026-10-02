//! Files pane: lazy workspace file tree (direct fs, local workspaces only)
//! with text / markdown preview (PARITY.md M3 "file tree + workspace file
//! preview").

use crate::shared::theme::{BORDER, CARD, HOVER, MUTED, TEXT, TOOL};
use crate::app::root::RootView;
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};
use std::path::{Path, PathBuf};

const MAX_PREVIEW_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug)]
pub struct FilesNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// `None` until the directory is expanded once (lazy loading).
    pub children: Option<Vec<FilesNode>>,
}

/// What the preview pane shows for the selected file.
#[derive(Clone, Debug)]
pub enum Preview {
    Markdown(String),
    Text(String),
    Binary,
    TooLarge(u64),
    Error(String),
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
}

impl RootView {
    pub(crate) fn ensure_files_loaded(&mut self, cx: &mut Context<Self>) {
        let active = self.active_workspace_path(cx);
        if active.is_none() || active == self.files.root {
            return;
        }
        self.files = FilesState {
            root: active,
            ..FilesState::default()
        };
        self.load_dir(None, cx);
    }

    /// Load a directory's children in the background (`None` = workspace root).
    fn load_dir(&mut self, dir: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(root) = self.files.root.clone() else {
            return;
        };
        self.files.loading = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let moved_dir = dir.clone();
            let list_root = root.clone();
            let entries = cx
                .background_spawn(async move { list_dir(&list_root, moved_dir.as_deref()) })
                .await;
            this.update(cx, |v, cx| {
                // The pane moved to another workspace while listing.
                if v.files.root.as_ref() != Some(&root) {
                    return;
                }
                v.files.loading = false;
                match dir {
                    None => v.files.roots = entries,
                    Some(d) => {
                        insert_children(&mut v.files.roots, &d, entries);
                        v.files.expanded.insert(d);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.files.expanded.remove(&path) {
            cx.notify();
            return;
        }
        self.load_dir(Some(path), cx);
    }

    pub(crate) fn select_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.files.selected = Some(path.clone());
        self.files.preview = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let preview = cx.background_spawn(async move { read_preview(&read_path) }).await;
            this.update(cx, |v, cx| {
                // Rapid clicks: an older, slower read must not replace the
                // preview of the file selected since.
                if v.files.selected.as_ref() != Some(&path) {
                    return;
                }
                v.files.preview = Some(preview);
                cx.notify();
            })
            .ok();
        })
        .detach();
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
                if n.is_dir && expanded.contains(&n.path)
                    && let Some(children) = &n.children {
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
                        el.child(pane_note("Loading…"))
                    })
                    .when(self.files.roots.is_empty() && !self.files.loading, |el| {
                        el.child(pane_note("No workspace"))
                    })
                    .children(rows.into_iter().map(|(depth, node)| {
                        self.files_row(depth, node, cx)
                    })),
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
                        (None, _) => pane_note("Select a file to preview"),
                        (Some(_), None) => pane_note("Loading…"),
                        (Some(p), Some(pv)) => preview_element(p, pv),
                    }),
            )
            .into_any_element()
    }

    fn files_row(
        &self,
        depth: usize,
        node: &FilesNode,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.files.selected.as_ref() == Some(&node.path);
        let expanded = self.files.expanded.contains(&node.path);
        let name = node.name.clone();
        let click_path = node.path.clone();
        let is_dir = node.is_dir;
        div()
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
                if is_dir {
                    this.toggle_dir(click_path.clone(), cx);
                } else {
                    this.select_file(click_path.clone(), cx);
                }
            }))
            .child(
                div()
                    .w(px(10.))
                    .text_size(px(9.))
                    .text_color(rgb(MUTED))
                    .child(if is_dir {
                        if expanded { "▾" } else { "▸" }.to_string()
                    } else {
                        String::new()
                    }),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .truncate()
                    .text_color(rgb(if node.is_dir { TOOL } else { TEXT }))
                    .child(name),
            )
            .into_any_element()
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

fn pane_note(text: &str) -> AnyElement {
    div()
        .p_3()
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .child(text.to_string())
        .into_any_element()
}

fn preview_element(path: &Path, preview: &Preview) -> AnyElement {
    let header = div()
        .px_2()
        .py_1()
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(path.display().to_string());
    let body: AnyElement = match preview {
        Preview::Error(e) => pane_note(&format!("Cannot read: {e}")),
        Preview::Binary => pane_note("Binary file"),
        Preview::TooLarge(size) => pane_note(&format!("File too large ({size} bytes)")),
        Preview::Text(text) => div()
            .p_2()
            .font_family("Consolas")
            .text_size(px(11.5))
            .child(text.clone())
            .into_any_element(),
        Preview::Markdown(text) => div()
            .p_2()
            .child(crate::shared::markdown::render_markdown(text, 0xF11E5, false))
            .into_any_element(),
    };
    div().flex().flex_col().min_h_0().child(header).child(body).into_any_element()
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

/// Read a file for preview, sniffing binary content and markdown extension.
fn read_preview(path: &Path) -> Preview {
    let Ok(meta) = std::fs::metadata(path) else {
        return Preview::Error("not found".into());
    };
    if meta.len() > MAX_PREVIEW_BYTES {
        return Preview::TooLarge(meta.len());
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            if bytes.contains(&0) {
                return Preview::Binary;
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            match path.extension().and_then(|e| e.to_str()) {
                Some("md") | Some("markdown") => Preview::Markdown(text),
                _ => Preview::Text(text),
            }
        }
        Err(e) => Preview::Error(e.to_string()),
    }
}
