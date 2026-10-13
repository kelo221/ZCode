//! Compiled backend: the unchanged CLI app-server built into one Bun
//! executable by `scripts/build_backend.ts` and shipped beside the GPUI
//! binary (`package-gpui --with-backend`). A packaged GPUI then needs neither
//! a repository checkout nor the installed Desktop runtime.
//!
//! Layout (all siblings of the GPUI executable):
//!   zcode-backend[.exe]
//!   provider/zcode-builtin.json   (the CLI resolves it beside its entry)
//!   node_modules/ripgrep/         (optional; without it text search falls
//!                                  back to the slower JavaScript engine)
use std::path::{Path, PathBuf};

use crate::backend::launcher::{BackendLaunch, common_backend_envs, find_installed_bundle};

pub(crate) const BACKEND_EXE: &str = if cfg!(windows) {
    "zcode-backend.exe"
} else {
    "zcode-backend"
};

/// The compiled backend next to the running GPUI executable, if complete.
pub(crate) fn sibling_launch() -> Option<BackendLaunch> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    launch_in(&dir)
}

pub(crate) fn launch_in(dir: &Path) -> Option<BackendLaunch> {
    let exe = ready_backend(dir)?;
    let mut envs = common_backend_envs();
    push_installed_rg(&mut envs);
    Some(BackendLaunch {
        program: exe.to_string_lossy().into_owned(),
        args: vec![
            "app-server".into(),
            "--stdio".into(),
            "--surface".into(),
            "desktop".into(),
        ],
        envs,
        describe: "compiled backend beside GPUI".into(),
    })
}

/// The executable alone is not enough: without the staged provider release
/// the CLI exits during startup, so an incomplete copy is skipped instead.
fn ready_backend(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join(BACKEND_EXE);
    let provider = dir.join("provider").join("zcode-builtin.json");
    (exe.is_file() && provider.is_file()).then_some(exe)
}

/// Reuse the Desktop install's native ripgrep for shell tooling when present
/// and the caller has not chosen one.
pub(crate) fn push_installed_rg(envs: &mut Vec<(String, String)>) {
    if std::env::var("ZCODE_RG_BINARY").is_ok() {
        return;
    }
    let Some((_, cjs)) = find_installed_bundle() else {
        return;
    };
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

#[cfg(test)]
#[path = "compiled_backend_tests.rs"]
mod tests;
