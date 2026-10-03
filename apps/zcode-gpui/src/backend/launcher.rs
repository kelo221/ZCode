//! Backend launcher: resolves and spawns the ZCode agent backend
//! (`app-server --stdio`) and pumps its stdio.
//!
//! The backend is an external process speaking NDJSON on stdin/stdout. This
//! frontend never links backend code, so upstream backend updates flow in
//! unchanged; only the protocol version on the wire matters.
//!
//! Resolution order (first spawnable candidate wins; the caller falls back to
//! the next candidate if a process dies before storage startup completes):
//!   1. `ZCODE_GPUI_AGENT_PROGRAM` + optional `ZCODE_GPUI_AGENT_ARGS` env
//!      override (debug/testing escape hatch).
//!   2. bun + this branch's sources (apps/zcode-cli/packages/cli/src/main.ts),
//!      the preferred runtime on this branch. Only offered when the workspace
//!      is installed (`bun install`) and the CLI packages are built, because
//!      the CLI packages export `dist/`; otherwise it would die at startup.
//!   3. The installed ZCode desktop app's bundled runtime
//!      (`zcode.cjs` under ELECTRON_RUN_AS_NODE=1), the same launch contract
//!      packages/services/src/zcode-agent/zcodeAgentProcessManager.ts uses for
//!      packaged builds. Fallback; it updates with the desktop app.
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct BackendLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
    pub describe: String,
}

pub fn resolve_candidates(workspace: &Path) -> Vec<BackendLaunch> {
    let mut out = Vec::new();

    // Only the GPUI-specific variable overrides: the desktop app exports
    // `GLM_BINARY_PATH` / `ZCODE_AGENT_SERVER_COMMAND` to every child process,
    // so honouring them would silently bypass bun when launched from a ZCode
    // terminal.
    if let Ok(program) = std::env::var("ZCODE_GPUI_AGENT_PROGRAM") {
        let args = std::env::var("ZCODE_GPUI_AGENT_ARGS")
            .map(|a| a.split_whitespace().map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_else(|_| {
                vec![
                    "app-server".into(),
                    "--stdio".into(),
                    "--surface".into(),
                    "desktop".into(),
                ]
            });
        out.push(BackendLaunch {
            program,
            args,
            envs: common_backend_envs(),
            describe: "env override".into(),
        });
    }

    if let Some(launch) = bun_source_launch(workspace) {
        out.push(launch);
    }
    if let Some(launch) = installed_app_launch() {
        out.push(launch);
    }

    out
}

fn repo_root_candidates(workspace: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |dir: &Path| {
        let mut cur = Some(dir.to_path_buf());
        while let Some(d) = cur {
            roots.push(d.clone());
            cur = d.parent().map(Path::to_path_buf);
        }
    };
    push(workspace);
    if let Ok(cwd) = std::env::current_dir() {
        push(&cwd);
    }
    roots
}

/// Files that must exist before bun can run the CLI from source: the
/// workspace install (root `node_modules` with the `@zcode/*` links) and the
/// built `dist/` of the packages the CLI entry imports.
pub(crate) fn bun_source_ready(root: &Path) -> bool {
    let cli = root.join("apps").join("zcode-cli").join("packages");
    [
        root.join("node_modules")
            .join("@zcode")
            .join("shared")
            .join("package.json"),
        cli.join("cli").join("src").join("main.ts"),
        cli.join("core").join("dist").join("index.js"),
        cli.join("bootstrap").join("dist").join("index.js"),
        cli.join("adapters").join("dist").join("index.js"),
    ]
    .iter()
    .all(|p| p.exists())
}

/// Absolute bun executable: `BUN_INSTALL`, then `PATH`, then `~/.bun/bin`.
/// Resolved up front so a GUI launch without the shell's PATH still works.
fn find_bun() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "bun.exe" } else { "bun" };
    let from_install = std::env::var_os("BUN_INSTALL").map(|d| PathBuf::from(d).join("bin"));
    let from_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let from_home = ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|h| PathBuf::from(h).join(".bun").join("bin"));
    from_install
        .into_iter()
        .chain(from_path)
        .chain(from_home)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

fn bun_source_launch(workspace: &Path) -> Option<BackendLaunch> {
    let root = repo_root_candidates(workspace)
        .into_iter()
        .find(|r| bun_source_ready(r))?;
    let bun = find_bun()?;
    let main = root
        .join("apps")
        .join("zcode-cli")
        .join("packages")
        .join("cli")
        .join("src")
        .join("main.ts");
    let mut envs = common_backend_envs();
    // Reuse the desktop install's ripgrep when present; the provider config
    // is resolved by the CLI itself so it matches this branch's version.
    if let Some((_, cjs)) = find_installed_bundle()
        && std::env::var("ZCODE_RG_BINARY").is_err()
    {
        let rg_name = if cfg!(windows) { "rg.exe" } else { "rg" };
        let rg = cjs
            .parent()
            .and_then(Path::parent)
            .map(|res| res.join("tools").join("ripgrep").join(rg_name))
            .filter(|p| p.exists());
        if let Some(rg) = rg {
            envs.push(("ZCODE_RG_BINARY".into(), rg.to_string_lossy().into_owned()));
        }
    }
    Some(BackendLaunch {
        program: bun.to_string_lossy().into_owned(),
        args: vec![
            main.to_string_lossy().into_owned(),
            "app-server".into(),
            "--stdio".into(),
            "--surface".into(),
            "desktop".into(),
        ],
        envs,
        describe: "bun + branch source".into(),
    })
}

fn find_installed_bundle() -> Option<(PathBuf, PathBuf)> {
    #[cfg(target_os = "windows")]
    {
        let mut app_dirs = Vec::new();
        for key in [
            "LOCALAPPDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
        ] {
            if let Ok(v) = std::env::var(key) {
                let sub = if key == "LOCALAPPDATA" {
                    "Programs\\ZCode"
                } else {
                    "ZCode"
                };
                app_dirs.push(PathBuf::from(v).join(sub));
            }
        }
        if let Ok(v) = std::env::var("USERPROFILE") {
            app_dirs.push(PathBuf::from(v).join("AppData\\Local\\Programs\\ZCode"));
        }
        for dir in app_dirs {
            let app = dir.join("ZCode.exe");
            let cjs = dir.join("resources").join("glm").join("zcode.cjs");
            if app.exists() && cjs.exists() {
                return Some((app, cjs));
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let mut roots = vec![PathBuf::from("/Applications/ZCode.app")];
        if let Ok(home) = std::env::var("HOME") {
            roots.push(PathBuf::from(home).join("Applications/ZCode.app"));
        }
        for app_dir in roots {
            let app = app_dir.join("Contents/MacOS/ZCode");
            let cjs = app_dir.join("Contents/Resources/glm/zcode.cjs");
            if app.exists() && cjs.exists() {
                return Some((app, cjs));
            }
        }
        None
    }
    #[cfg(target_os = "linux")]
    {
        let candidates = [
            (
                PathBuf::from("/opt/ZCode/zcode"),
                PathBuf::from("/opt/ZCode/resources/glm/zcode.cjs"),
            ),
            (
                PathBuf::from("/usr/share/zcode/zcode"),
                PathBuf::from("/usr/share/zcode/resources/glm/zcode.cjs"),
            ),
        ];
        for (app, cjs) in candidates {
            if app.exists() && cjs.exists() {
                return Some((app, cjs));
            }
        }
        None
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        not(target_os = "linux")
    ))]
    {
        None
    }
}

fn common_backend_envs() -> Vec<(String, String)> {
    let mut envs = vec![("ZCODE_RUNTIME_ENV".into(), "desktop".into())];
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "ZCODE_DATA_BASE_DIR",
        "ZCODE_HOME",
    ] {
        if let Ok(val) = std::env::var(key) {
            envs.push((key.into(), val));
        }
    }
    envs
}

fn installed_app_launch() -> Option<BackendLaunch> {
    let (app, cjs) = find_installed_bundle()?;
    let mut envs = common_backend_envs();
    envs.push(("ELECTRON_RUN_AS_NODE".into(), "1".into()));

    if let Some(res) = cjs.parent().and_then(|p| p.parent()) {
        let cfg = res
            .join("config")
            .join("provider")
            .join("zcode-builtin.json");
        let glm = res.join("glm").join("provider").join("zcode-builtin.json");

        let builtin = if cfg.exists() {
            Some(cfg)
        } else if glm.exists() {
            Some(glm)
        } else {
            None
        };
        if let Some(b) =
            builtin.filter(|_| std::env::var("ZCODE_BUILTIN_PROVIDER_CONFIG_FILE").is_err())
        {
            envs.push((
                "ZCODE_BUILTIN_PROVIDER_CONFIG_FILE".into(),
                b.to_string_lossy().into_owned(),
            ));
        }

        let rg =
            res.join("tools")
                .join("ripgrep")
                .join(if cfg!(windows) { "rg.exe" } else { "rg" });
        if rg.exists() && std::env::var("ZCODE_RG_BINARY").is_err() {
            envs.push(("ZCODE_RG_BINARY".into(), rg.to_string_lossy().into_owned()));
        }
    }

    Some(BackendLaunch {
        program: app.to_string_lossy().into_owned(),
        args: vec![
            cjs.to_string_lossy().into_owned(),
            "app-server".into(),
            "--stdio".into(),
            "--surface".into(),
            "desktop".into(),
        ],
        envs,
        describe: "installed ZCode app runtime".into(),
    })
}

pub use crate::backend::conn::{ConnEvent, spawn_connection};

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_command_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
#[path = "launcher_tests.rs"]
mod tests;
