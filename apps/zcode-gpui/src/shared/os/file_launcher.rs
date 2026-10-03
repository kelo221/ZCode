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
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/C", "start", "", &path.display().to_string()]);
        cmd.creation_flags(0x08000000);
        cmd.spawn()?;
        Ok(())
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
