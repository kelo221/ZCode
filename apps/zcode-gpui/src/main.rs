//! zcode-gpui: minimal native GPUI frontend for the ZCode agent backend.
//!
//! Chat + project/sessions view only; settings and heavier features stay in
//! the original Electron app. The backend is spawned as an external process
//! (`app-server --stdio`) and driven over the V4 wire protocol, so backend
//! updates from upstream keep flowing without frontend changes.

mod catalog;
mod chat;
mod commands;
mod composer;
mod config_cmds;
mod events;
mod interaction_view;
mod interactions;
mod launcher;
mod menus;
mod model;
mod msg_actions;
mod queue;
mod responses;
mod rows;
mod session_cmds;
mod sidebar;
mod store;
mod theme;
mod turn_meta;
mod ui;
mod ui_parts;
mod wire;
mod workspace;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use launcher::resolve_candidates;
use std::path::PathBuf;
use store::AppState;
use ui::RootView;
use workspace::discover_workspaces;

fn main() {
    let mut workspace: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--workspace" => workspace = args.next().map(PathBuf::from),
            other => {
                if workspace.is_none() {
                    workspace = Some(PathBuf::from(other));
                }
            }
        }
    }
    let workspace = workspace
        .and_then(|p| p.canonicalize().ok())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    Application::new().run(move |cx: &mut App| {
        // One agent per known workspace (desktop parity: processes are keyed by
        // workspace key; the project list comes from ~/.zcode/v2/setting.json).
        let workspaces = discover_workspaces(&workspace, 8);
        let candidates = resolve_candidates(&workspace);
        let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("ZCode (GPUI)".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_window, cx| {
                let state = cx.new(|cx| AppState::new(workspaces, candidates, cx));
                cx.new(|cx| RootView::new(state, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
