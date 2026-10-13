//! Services Host launch resolution (first available wins):
//!   1. `ZCODE_GPUI_SERVICES_RUNTIME` / `ZCODE_GPUI_SERVICES_BUNDLE` overrides:
//!      the built stdio bundle under an Electron-compatible runtime.
//!   2. bun + this checkout's source via `scripts/services_host.ts`.
//!   3. The built bundle under the installed ZCode Desktop runtime.
use crate::backend::launcher::{
    BackendLaunch, common_backend_envs, find_bun, find_installed_bundle,
};
use crate::shared::isolation::IsolatedSettings;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub(crate) fn resolve(isolated: &IsolatedSettings) -> Result<BackendLaunch, String> {
    resolve_mode(Some(isolated))
}

pub(crate) fn resolve_mode(isolated: Option<&IsolatedSettings>) -> Result<BackendLaunch, String> {
    let overridden = std::env::var_os("ZCODE_GPUI_SERVICES_RUNTIME").is_some()
        || std::env::var_os("ZCODE_GPUI_SERVICES_BUNDLE").is_some();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut launch = match (!overridden).then(|| bun_source_launch(source)).flatten() {
        Some(launch) => launch,
        None => bundle_launch(source)?,
    };
    if let Some(isolated) = isolated {
        isolated.apply_child_environment(&mut launch.envs);
    }
    let mode = if isolated.is_some() {
        "disposable Settings mode"
    } else {
        "local management"
    };
    launch.describe = format!("{} ({mode})", launch.describe);
    Ok(launch)
}

fn bun_source_launch(start: &Path) -> Option<BackendLaunch> {
    let root = start.ancestors().find(|root| {
        root.join("packages/server/src/entry-stdio.ts").is_file()
            && root
                .join("node_modules/@zcode/shared/package.json")
                .is_file()
    })?;
    let script = root.join("apps/zcode-gpui/scripts/services_host.ts");
    let bun = find_bun().filter(|_| script.is_file())?;
    let mut envs = common_backend_envs();
    if std::env::var_os("ZCODE_BUILTIN_PROVIDER_CONFIG_FILE").is_none()
        && let Some(config) = installed_builtin_provider_config()
    {
        envs.push((
            "ZCODE_BUILTIN_PROVIDER_CONFIG_FILE".into(),
            config.to_string_lossy().into_owned(),
        ));
    }
    Some(BackendLaunch {
        program: bun.to_string_lossy().into_owned(),
        args: vec![script.to_string_lossy().into_owned()],
        envs,
        describe: "Services Host via bun + branch source".into(),
    })
}

/// The installed Desktop's production Built-in Provider release, so the shared
/// materialized copy matches what Desktop itself writes.
fn installed_builtin_provider_config() -> Option<PathBuf> {
    let (_, cjs) = find_installed_bundle()?;
    let resources = cjs.parent()?.parent()?;
    [
        resources.join("config/provider/zcode-builtin.json"),
        resources.join("glm/provider/zcode-builtin.json"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn bundle_launch(source: &Path) -> Result<BackendLaunch, String> {
    let bundle = std::env::var_os("ZCODE_GPUI_SERVICES_BUNDLE")
        .map(PathBuf::from)
        .or_else(|| repository_bundle(source))
        .filter(|path| path.is_file())
        .ok_or(
            "Services Host is unavailable: install bun and the workspace (bun install), \
             or build the existing server bundle",
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
        describe: "Services Host via built bundle".into(),
    })
}

fn repository_bundle(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|root| root.join("packages/server/dist/remote/zcode-server.cjs"))
        .find(|candidate| candidate.is_file())
}
