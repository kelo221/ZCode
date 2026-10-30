//! OS launcher for opening files in editors (VS Code / default) and revealing
//! files in native file managers (Windows Explorer, Finder, xdg-open).

#![allow(dead_code)]

use gpui::{App, AppContext};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Open `path` with the OS default handler; `on_error` runs on the UI thread.
pub fn open_with_system_default(
    path: PathBuf,
    cx: &App,
    on_error: impl FnOnce(std::io::Error, &mut App) + 'static,
) {
    run_off_ui_thread(
        cx,
        move || open_with_system_default_blocking(&path),
        on_error,
    );
}

/// Open `path` in VS Code, falling back to the OS default handler; `on_error`
/// runs on the UI thread.
pub fn open_in_editor(
    path: PathBuf,
    cx: &App,
    on_error: impl FnOnce(std::io::Error, &mut App) + 'static,
) {
    run_off_ui_thread(cx, move || open_in_editor_blocking(&path, None), on_error);
}

/// Open `path` at `line` in VS Code (`code -g`), falling back to the OS
/// default handler (which ignores the line).
pub fn open_in_editor_at(
    path: PathBuf,
    line: u32,
    cx: &App,
    on_error: impl FnOnce(std::io::Error, &mut App) + 'static,
) {
    run_off_ui_thread(
        cx,
        move || open_in_editor_blocking(&path, Some(line)),
        on_error,
    );
}

// 不能在 UI 线程调用 ShellExecuteW：它会跑一个嵌套的 Win32 消息循环（打开文件夹时
// 必现），把 GPUI 排队的前台任务在点击回调仍持有 App 借用时派发出去；该任务再
// borrow_mut 就会 "RefCell already borrowed"，且 panic 发生在 WndProc 内不能 unwind，
// 进程直接 abort（2026-10-22 侧边栏项目菜单"在文件资源管理器中打开"崩溃）。
// 与上游 GPUI 的 open_url / open_with_system 一样放到后台执行器上执行。
fn run_off_ui_thread(
    cx: &App,
    job: impl FnOnce() -> Result<(), std::io::Error> + Send + 'static,
    on_error: impl FnOnce(std::io::Error, &mut App) + 'static,
) {
    let task = cx.background_spawn(async move { job() });
    cx.spawn(async move |cx| {
        if let Err(error) = task.await {
            cx.update(|cx| on_error(error, cx));
        }
    })
    .detach();
}

/// Tries `code <path>` first, then the default OS opener. Blocks; never call
/// on the UI thread (see `run_off_ui_thread`).
fn open_in_editor_blocking(path: &Path, line: Option<u32>) -> Result<(), std::io::Error> {
    // 1. Try VS Code if available on PATH
    let mut code_cmd = Command::new("code");
    if let Some(line) = line {
        let mut target = path.as_os_str().to_owned();
        target.push(format!(":{line}"));
        code_cmd.arg("-g").arg(target);
    } else {
        code_cmd.arg(path);
    }
    #[cfg(windows)]
    code_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    if code_cmd.spawn().is_ok() {
        return Ok(());
    }

    // 2. Fallback to default OS file opener
    open_with_system_default_blocking(path)
}

/// Reveal a file or directory in the native file manager with selection.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer.exe");
        let arg = format!("/select,{}", path.display());
        cmd.arg(arg);
        cmd.spawn()?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg("-R").arg(path);
        cmd.spawn()?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let parent = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let mut cmd = Command::new("xdg-open");
        cmd.arg(parent);
        cmd.spawn()?;
        Ok(())
    }
}

/// Blocks (and on Windows pumps a nested message loop); only call from a
/// background executor. UI code uses [`open_with_system_default`].
pub(crate) fn open_with_system_default_blocking(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(windows)]
    {
        // Direct ShellExecuteW — never `cmd.exe /C start`: workspace paths can
        // contain shell metacharacters, and quoting rules around `start` are
        // notoriously lossy (2026-10-05 audit P0.7).
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::w;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let hinstance = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                windows::core::PCWSTR(wide.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a value > 32 on success.
        let code = hinstance.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "ShellExecuteW failed with code {code}"
            )))
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd.spawn()?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path);
        cmd.spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launcher_helpers_construct() {
        let path = Path::new("test.txt");
        assert_eq!(path.to_str().unwrap(), "test.txt");
    }
}
