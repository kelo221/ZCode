//! zcode-gpui: minimal native GPUI frontend for the ZCode agent backend.
//!
//! Chat + project/sessions view only; settings and heavier features stay in
//! the original Electron app. The backend is spawned as an external process
//! (`app-server --stdio`) and driven over the V4 wire protocol, so backend
//! updates from upstream keep flowing without frontend changes.
//!
//! Source is organised as vertical slices: each folder owns one feature end
//! to end (state, effects and views). `app` composes the slices into the
//! window; `shared` holds the theme and renderers several slices reuse.

mod app;
mod backend;
mod composer;
mod conversation;
mod files;
mod review;
mod sessions;
mod shared;
mod terminal;
mod transcript;

use app::dock::ToggleDock;
use app::root::RootView;
use app::store::AppState;
use backend::launcher::resolve_candidates;
use backend::workspace::discover_workspaces;
use gpui::{
    App, AppContext, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions,
    px, size,
};
use std::path::PathBuf;
use terminal::pane::ToggleTerminal;

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

fn main() {
    // GUI panics vanish with the window; persist them for diagnosis.
    let panic_log = std::env::temp_dir().join("zcode-gpui-panic.log");
    // Keep the default hook so panics still reach stderr under `cargo run`.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&panic_log) {
            let _ = writeln!(f, "=== panic at {} ===", chrono_now());
            let _ = writeln!(f, "{info}");
            let _ = writeln!(f, "{:?}", std::backtrace::Backtrace::force_capture());
        }
        default_hook(info);
    }));

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
        cx.bind_keys([
            KeyBinding::new("ctrl-b", ToggleDock, None),
            KeyBinding::new("ctrl-`", ToggleTerminal, None),
        ]);
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
