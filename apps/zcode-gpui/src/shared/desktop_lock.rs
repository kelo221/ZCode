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
fn desktop_process_running(owned: &[u32]) -> bool {
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
        let mut entries = Vec::new();
        if Process32FirstW(snapshot, &mut entry).is_err() {
            let _ = CloseHandle(snapshot);
            return true;
        }
        loop {
            entries.push(entry);
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let entries = entries
            .into_iter()
            .map(|entry| {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                (
                    entry.th32ProcessID,
                    entry.th32ParentProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase(),
                )
            })
            .collect::<Vec<_>>();
        let _ = CloseHandle(snapshot);
        desktop_snapshot_running(&entries, my_pid, owned)
    }
}

#[cfg(any(windows, test))]
fn desktop_snapshot_running(entries: &[(u32, u32, String)], my_pid: u32, owned: &[u32]) -> bool {
    // CLI 与 Host 使用相同 Electron 可执行文件；仅排除 owned 子树，不能忽略其他桌面实例。
    let mut excluded = owned.to_vec();
    excluded.push(my_pid);
    for _ in 0..entries.len() {
        let before = excluded.len();
        for (pid, parent, _) in entries {
            if excluded.contains(parent) && !excluded.contains(pid) {
                excluded.push(*pid);
            }
        }
        if excluded.len() == before {
            break;
        }
    }
    entries
        .iter()
        .any(|(pid, _, name)| !excluded.contains(pid) && DESKTOP_EXE_NAMES.contains(&name.as_str()))
}

#[cfg(not(windows))]
fn desktop_process_running(_owned: &[u32]) -> bool {
    false
}

/// True when any ZCode desktop instance may be running.
pub fn is_desktop_running() -> bool {
    is_desktop_running_except(&[])
}

pub(crate) fn is_desktop_running_except(owned: &[u32]) -> bool {
    desktop_user_data_dirs()
        .iter()
        .any(|d| singleton_lock_held(d))
        || desktop_process_running(owned)
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
    fn owned_runtime_tree_is_excluded_without_ignoring_unowned_desktop() {
        let row = |pid, parent, name: &str| (pid, parent, name.to_owned());
        let mut snapshot = vec![
            row(10, 1, "zcode-gpui.exe"),
            row(11, 10, "zcode.exe"),
            row(12, 11, "zcode.exe"),
            row(20, 1, "zcode.exe"),
        ];
        assert!(!desktop_snapshot_running(&snapshot, 10, &[20]));
        assert!(desktop_snapshot_running(&snapshot, 10, &[]));
        snapshot.pop();
        assert!(!desktop_snapshot_running(&snapshot, 10, &[]));
        snapshot.push(row(30, 1, "zcode.exe"));
        assert!(desktop_snapshot_running(&snapshot, 10, &[20]));
    }

    #[test]
    fn exe_names_cover_every_product_identity() {
        for name in DESKTOP_APP_NAMES {
            let exe = format!("{}.exe", name.to_lowercase());
            assert!(DESKTOP_EXE_NAMES.contains(&exe.as_str()), "{exe}");
        }
    }
}
