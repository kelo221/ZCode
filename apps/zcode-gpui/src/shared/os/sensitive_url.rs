//! External sensitive-URL opening without target-bearing diagnostics or shell evaluation.
#[cfg(windows)]
pub(crate) fn open(url: &str) -> Result<(), ()> {
    crate::shared::os::file_launcher::open_with_system_default(std::path::Path::new(url))
        .map_err(|_| ())
}

#[cfg(not(windows))]
pub(crate) fn open(url: &str) -> Result<(), ()> {
    use std::process::{Command, Stdio};
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    // GPUI 原生 opener 的失败日志可能包含 OAuth token；直接启动且丢弃子进程输出。
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| ())
        .and_then(|status| status.success().then_some(()).ok_or(()))
}
