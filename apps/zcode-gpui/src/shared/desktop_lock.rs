//! Detect a running ZCode desktop through its own single-instance lock.
//!
//! The desktop calls Electron's `app.requestSingleInstanceLock()` with
//! `userData = <appData>/<runtimeApplicationName>`
//! (`packages/desktop/src/main/desktopRuntimeEnv.ts`). Electron implements it
//! with Chromium's ProcessSingleton, which keeps:
//! - Windows: `<userData>/lockfile` open for writing with share mode
//!   `FILE_SHARE_READ` and `FILE_FLAG_DELETE_ON_CLOSE`;
//! - macOS/Linux: a `<userData>/SingletonLock` symlink.
//!
//! Any doubt resolves to "running": the caller only uses this to refuse
//! writes, and a refused write is recoverable while a clobbered store is not.

use std::path::{Path, PathBuf};

/// `runtimeApplicationName` values: packaged, preview and local dev builds.
const DESKTOP_APP_NAMES: &[&str] = &["ZCode", "ZCode Preview", "ZCode Dev"];

/// Executable names of packaged desktop builds (`productName` + `.exe`).
pub(crate) const DESKTOP_EXE_NAMES: &[&str] = &["zcode.exe", "zcode preview.exe", "zcode dev.exe"];

fn app_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }
}

/// Every userData directory a desktop instance may lock.
pub(crate) fn desktop_user_data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(over) = std::env::var_os("ZCODE_DESKTOP_USER_DATA_DIR")
        && !over.is_empty()
    {
        dirs.push(PathBuf::from(over));
    }
    if let Some(base) = app_data_dir() {
        dirs.extend(DESKTOP_APP_NAMES.iter().map(|n| base.join(n)));
    }
    dirs
}

/// Whether the ProcessSingleton lock in `user_data` is currently held.
pub(crate) fn singleton_lock_held(user_data: &Path) -> bool {
    #[cfg(windows)]
    {
        let lock = user_data.join("lockfile");
        if !lock.exists() {
            return false;
        }
        // Write access without create/truncate: never modifies the file.
        // The live holder only shares reads, so this fails while it runs;
        // success means a stale file left behind by a crash.
        std::fs::OpenOptions::new().write(true).open(&lock).is_err()
    }
    #[cfg(not(windows))]
    {
        // The symlink is removed on clean exit; a stale one (crash) is
        // treated as held, which only blocks writes.
        user_data.join("SingletonLock").symlink_metadata().is_ok()
    }
}

#[cfg(windows)]
fn desktop_process_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            // Cannot enumerate: assume running (fail closed for writes).
            return true;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let my_pid = std::process::id();
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID != my_pid {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let exe = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                    if DESKTOP_EXE_NAMES.contains(&exe.as_str()) {
                        found = true;
                        break;
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
fn desktop_process_running() -> bool {
    false
}

/// True when any ZCode desktop instance may be running.
pub fn is_desktop_running() -> bool {
    desktop_user_data_dirs()
        .iter()
        .any(|d| singleton_lock_held(d))
        || desktop_process_running()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zcode_gpui_lock_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_lock_is_not_held() {
        let d = scratch("missing");
        assert!(!singleton_lock_held(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn lockfile_held_with_read_only_sharing_is_detected() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        let d = scratch("held");
        let lock = d.join("lockfile");
        // Mirror Chromium: open for write, share reads only.
        let holder = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(FILE_SHARE_READ)
            .open(&lock)
            .unwrap();
        assert!(singleton_lock_held(&d));
        drop(holder);
        // Stale file after a crash: not held.
        assert!(!singleton_lock_held(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn exe_names_cover_every_product_identity() {
        for name in DESKTOP_APP_NAMES {
            let exe = format!("{}.exe", name.to_lowercase());
            assert!(DESKTOP_EXE_NAMES.contains(&exe.as_str()), "{exe}");
        }
    }
}
