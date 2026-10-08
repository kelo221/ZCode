use crate::backend::launcher::{BackendLaunch, common_backend_envs, find_installed_bundle};
use crate::shared::isolation::IsolatedSettings;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(crate) fn resolve(isolated: &IsolatedSettings) -> Result<BackendLaunch, String> {
    resolve_mode(Some(isolated))
}

pub(crate) fn resolve_mode(isolated: Option<&IsolatedSettings>) -> Result<BackendLaunch, String> {
    let bundle = std::env::var_os("ZCODE_GPUI_SERVICES_BUNDLE")
        .map(PathBuf::from)
        .or_else(|| repository_bundle(Path::new(env!("CARGO_MANIFEST_DIR"))))
        .filter(|path| path.is_file())
        .ok_or(
            "Unchanged Services stdio bundle is unavailable; build the existing server bundle",
        )?;
    let program = if let Some(runtime) = std::env::var_os("ZCODE_GPUI_SERVICES_RUNTIME") {
        PathBuf::from(runtime)
    } else if let Some((runtime, _)) = find_installed_bundle() {
        runtime
    } else {
        return Err(
            "Compatible Services runtime is unavailable; no runtime will be installed".into(),
        );
    };
    if !program.is_file() {
        return Err("Services runtime executable is unavailable".into());
    }
    let mut envs = common_backend_envs();
    if let Some(isolated) = isolated {
        isolated.apply_child_environment(&mut envs);
    }
    // 显式 runtime 也可能是 Electron；该开关让它执行已有 Node 入口，普通 Node 会忽略它。
    envs.retain(|(key, _)| !key.eq_ignore_ascii_case("ELECTRON_RUN_AS_NODE"));
    envs.push(("ELECTRON_RUN_AS_NODE".into(), "1".into()));
    Ok(BackendLaunch {
        program: program.to_string_lossy().into_owned(),
        args: vec![
            "--no-warnings".into(),
            bundle.to_string_lossy().into_owned(),
        ],
        envs,
        describe: if isolated.is_some() {
            "unchanged Services Host (disposable Settings mode)"
        } else {
            "unchanged Services Host (local management)"
        }
        .into(),
    })
}

fn repository_bundle(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|root| root.join("packages/server/dist/remote/zcode-server.cjs"))
        .find(|candidate| candidate.is_file())
}
