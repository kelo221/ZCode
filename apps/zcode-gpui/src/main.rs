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
use gpui::{App, AppContext, Application, KeyBinding, TitlebarOptions, WindowOptions};
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
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&panic_log)
        {
            let _ = writeln!(f, "=== panic at {} ===", chrono_now());
            // Panic payloads can format arbitrary state; never persist secrets.
            let _ = writeln!(f, "{}", shared::redact::scrub(&info.to_string()));
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

    // M5: Load settings, locale, theme, and font size on launch
    let settings = shared::settings::load_settings();
    if let Some(loc_str) = settings
        .locale_preference
        .as_deref()
        .or(settings.locale.as_deref())
    {
        let pref = shared::i18n::LocalePreference::parse(loc_str);
        shared::i18n::set_current_locale(pref.resolve());
    }
    if let Some(theme_str) = settings
        .theme_preference
        .as_deref()
        .or(settings.theme.as_deref())
    {
        let mode = shared::theme::ThemeMode::parse(theme_str);
        shared::theme::set_theme_mode(mode);
    }
    if let Some(font_size) = settings.ui_font_size {
        shared::theme::set_ui_font_size(font_size);
    }

    // M6: Single-instance guard and forward-to-first launch request
    let instance_msg = shared::os::single_instance::InstanceMessage {
        action: "activate".into(),
        workspace: Some(workspace.to_string_lossy().into_owned()),
    };
    let _instance_guard = match shared::os::single_instance::try_acquire_single_instance(
        "Local\\ZCodeGPUI_SingleInstance_Mutex",
        instance_msg,
        |_msg| {
            // Primary instance received launch message from secondary instance
        },
    ) {
        shared::os::single_instance::InstanceRole::Secondary => {
            // Already forwarded to running primary instance, exit cleanly
            return;
        }
        shared::os::single_instance::InstanceRole::Primary(guard) => guard,
    };

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("ctrl-b", ToggleDock, None),
            KeyBinding::new("ctrl-`", ToggleTerminal, None),
            KeyBinding::new("ctrl-j", ToggleTerminal, None),
            KeyBinding::new("ctrl-k", app::quickpick::ToggleQuickPick, None),
            KeyBinding::new("ctrl-shift-p", app::quickpick::ToggleQuickPick, None),
            KeyBinding::new("ctrl-shift-l", app::quickpick::SwitchThemeAction, None),
        ]);
        // One agent per known workspace (desktop parity: processes are keyed by
        // workspace key; the project list comes from ~/.zcode/v2/setting.json).
        let workspaces = discover_workspaces(&workspace, 8);
        let candidates = resolve_candidates(&workspace);
        let window_state = shared::window_state::load_window_state();
        let initial_bounds = window_state.to_window_bounds(cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(initial_bounds),
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
