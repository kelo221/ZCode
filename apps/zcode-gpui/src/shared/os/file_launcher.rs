//! OS launcher for opening files in editors (VS Code / default) and revealing
//! files in native file managers (Windows Explorer, Finder, xdg-open).

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Open a file in the user's editor. Tries `code <path>` first, then default OS opener.
pub fn open_in_editor(path: &Path) -> Result<(), std::io::Error> {
    // 1. Try VS Code if available on PATH
    let mut code_cmd = Command::new("code");
    code_cmd.arg(path);
    #[cfg(windows)]
    code_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    if code_cmd.spawn().is_ok() {
        return Ok(());
    }

    // 2. Fallback to default OS file opener
    open_with_system_default(path)
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

pub fn open_with_system_default(path: &Path) -> Result<(), std::io::Error> {
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
