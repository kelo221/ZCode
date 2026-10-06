//! Native directory picker (Electron parity: `PlatformChannels.SelectDirectory`
//! → `dialog.showOpenDialog({openDirectory, createDirectory})`).
//!
//! Windows runs `IFileDialog` with `FOS_PICKFOLDERS` on a dedicated STA
//! thread — never through a shell, and never on the UI thread, so the gpui
//! render loop keeps running while the modal dialog is up. macOS shells out
//! to `osascript` and Linux to zenity/kdialog; both report None when the
//! user closes the dialog.

use futures::channel::oneshot;
use std::path::PathBuf;

/// Spawn the picker and return a receiver for the result: `Ok(None)` means
/// the user cancelled, `Err(reason)` means no picker was available.
pub(crate) fn pick_directory() -> oneshot::Receiver<Result<Option<PathBuf>, String>> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(pick_blocking());
    });
    rx
}

fn pick_blocking() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    return pick_windows();
    #[cfg(target_os = "macos")]
    return pick_osascript();
    #[cfg(all(unix, not(target_os = "macos")))]
    return pick_zenity();
}

#[cfg(windows)]
fn pick_windows() -> Result<Option<PathBuf>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::UI::Shell::{FOS_PICKFOLDERS, IFileDialog, SIGDN_FILESYSPATH};

    // A fresh STA thread owns the whole dialog lifecycle; every failure path
    // returns a reason instead of panicking (no COM expects on the UI path).
    let _com =
        CoInit::new().ok_or_else(|| "CoInitializeEx failed for folder picker".to_string())?;
    unsafe {
        let dialog: IFileDialog = CoCreateInstance(
            &windows::Win32::UI::Shell::FileOpenDialog,
            None,
            CLSCTX_INPROC_SERVER,
        )
        .map_err(|e| format!("FileOpenDialog unavailable: {e}"))?;
        let options = dialog.GetOptions().unwrap_or_default();
        dialog
            .SetOptions(options | FOS_PICKFOLDERS)
            .map_err(|e| format!("SetOptions failed: {e}"))?;
        if let Err(e) = dialog.Show(None) {
            // User cancel arrives as a failed HRESULT (ERROR_CANCELLED).
            if e.code() != windows::core::HRESULT::from(ERROR_CANCELLED) {
                return Err(format!("folder dialog failed: {e}"));
            }
            return Ok(None);
        }
        let item = dialog
            .GetResult()
            .map_err(|e| format!("dialog result missing: {e}"))?;
        let pwstr = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("display name failed: {e}"))?;
        let path = pwstr.to_string().ok();
        CoTaskMemFree(Some(pwstr.0 as *const std::ffi::c_void));
        Ok(path.filter(|p| !p.is_empty()).map(PathBuf::from))
    }
}

/// RAII COM init for the picker thread. S_OK/S_FALSE both require a balancing
/// CoUninitialize; RPC_E_CHANGED_MODE means COM is already up in another
/// apartment mode — usable as-is, nothing to balance.
#[cfg(windows)]
struct CoInit(bool);

#[cfg(windows)]
impl CoInit {
    fn new() -> Option<Self> {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // RPC_E_CHANGED_MODE = 0x80010106.
        if hr.is_ok() {
            Some(Self(true))
        } else if hr == windows::core::HRESULT(0x8001_0106u32 as i32) {
            Some(Self(false))
        } else {
            None
        }
    }
}

#[cfg(windows)]
impl Drop for CoInit {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[cfg(target_os = "macos")]
fn pick_osascript() -> Result<Option<PathBuf>, String> {
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg("POSIX path of (choose folder)")
        .output()
        .map_err(|e| format!("osascript unavailable: {e}"))?;
    if !out.status.success() {
        return Ok(None); // user cancelled
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((!text.is_empty()).then(|| PathBuf::from(text)))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn pick_zenity() -> Result<Option<PathBuf>, String> {
    for cmd in [
        vec!["zenity", "--file-selection", "--directory"],
        vec!["kdialog", "--getexistingdirectory", "."],
    ] {
        let (program, args) = cmd.split_first().expect("non-empty");
        if let Ok(out) = std::process::Command::new(program).args(args).output() {
            if !out.status.success() {
                return Ok(None); // user cancelled
            }
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Ok(Some(PathBuf::from(text)));
            }
        }
    }
    Err("no folder picker available (install zenity or kdialog)".into())
}
