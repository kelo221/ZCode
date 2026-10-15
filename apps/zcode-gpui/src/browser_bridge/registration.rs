//! Registers `zcode-browser-host` as a native messaging host so the browser
//! may start it for the ZCode extension. Chromium looks the host name up in
//! the registry (Windows), which points at a JSON manifest naming the program
//! and the one extension origin allowed to start it.
//!
//! Only Windows is implemented; other systems report `Unsupported` until
//! their manifest directories are added.

use super::native::{EXTENSION_ID, HOST_NAME};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Google Chrome and Helium both read the Google Chrome key (verified with a
/// probe host); Chromium builds read their own.
pub(crate) const REGISTRY_PARENTS: &[&str] = &[
    r"Software\Google\Chrome\NativeMessagingHosts",
    r"Software\Chromium\NativeMessagingHosts",
];

#[cfg(windows)]
const HOST_EXE: &str = "zcode-browser-host.exe";
#[cfg(not(windows))]
const HOST_EXE: &str = "zcode-browser-host";

/// Where the browser finds the host program.
pub(crate) trait HostRegistry {
    fn supported(&self) -> bool;
    /// The manifest path registered under `parent\HOST_NAME`, if any.
    fn read(&self, parent: &str) -> Option<String>;
    fn write(&self, parent: &str, manifest: &Path) -> Result<(), String>;
    fn delete(&self, parent: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Registered {
    No,
    /// Registered and pointing at this GPUI's host program.
    Ours,
    /// Registered for another copy of GPUI (for example a debug build).
    Other(String),
    Unsupported,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RegistrationStatus {
    pub(crate) host: PathBuf,
    pub(crate) host_exists: bool,
    pub(crate) registered: Registered,
}

pub(crate) fn manifest_file(dir: &Path) -> PathBuf {
    dir.join(format!("{HOST_NAME}.json"))
}

pub(crate) fn manifest_json(host: &Path) -> String {
    let manifest = json!({
        "name": HOST_NAME,
        "description": "ZCode GPUI browser bridge",
        "path": host.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    });
    serde_json::to_string_pretty(&manifest).unwrap_or_default()
}

/// The host program ships next to the GPUI executable.
pub(crate) fn host_exe_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(HOST_EXE))
}

/// The unpacked extension: `browser-extension` next to the executable in a
/// packaged build, else the extension's build output in the checkout this
/// binary was built from (absent on other machines).
pub(crate) fn extension_dir() -> Option<PathBuf> {
    let packaged = std::env::current_exe()
        .ok()?
        .parent()?
        .join("browser-extension");
    if packaged.is_dir() {
        return Some(packaged);
    }
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../zcode-browser-extension/dist");
    checkout.is_dir().then_some(checkout)
}

fn manifest_host(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["path"].as_str().map(str::to_string)
}

fn same_path(a: &str, b: &Path) -> bool {
    let b = b.to_string_lossy();
    if cfg!(windows) {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

pub(crate) fn status(registry: &dyn HostRegistry, host: &Path) -> RegistrationStatus {
    let registered = if !registry.supported() {
        Registered::Unsupported
    } else {
        match registry.read(REGISTRY_PARENTS[0]) {
            None => Registered::No,
            Some(path) => {
                let manifest = PathBuf::from(&path);
                match manifest_host(&manifest) {
                    Some(target) if same_path(&target, host) => Registered::Ours,
                    Some(target) => Registered::Other(target),
                    None => Registered::Other(path),
                }
            }
        }
    };
    RegistrationStatus {
        host: host.to_path_buf(),
        host_exists: host.is_file(),
        registered,
    }
}

pub(crate) fn register(registry: &dyn HostRegistry, dir: &Path, host: &Path) -> Result<(), String> {
    if !registry.supported() {
        return Err("registering the browser host is only implemented on Windows".into());
    }
    if !host.is_file() {
        return Err(format!("{} is missing", host.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let manifest = manifest_file(dir);
    let tmp = manifest.with_extension("json.tmp");
    std::fs::write(&tmp, manifest_json(host)).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &manifest).map_err(|e| e.to_string())?;
    REGISTRY_PARENTS
        .iter()
        .try_for_each(|parent| registry.write(parent, &manifest))
}

pub(crate) fn unregister(registry: &dyn HostRegistry, dir: &Path) -> Result<(), String> {
    for parent in REGISTRY_PARENTS {
        registry.delete(parent)?;
    }
    match std::fs::remove_file(manifest_file(dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// The current user's registry (HKCU, no elevation needed).
pub(crate) struct SystemRegistry;

#[cfg(not(windows))]
impl HostRegistry for SystemRegistry {
    fn supported(&self) -> bool {
        false
    }
    fn read(&self, _: &str) -> Option<String> {
        None
    }
    fn write(&self, _: &str, _: &Path) -> Result<(), String> {
        Err("unsupported".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
impl HostRegistry for SystemRegistry {
    fn supported(&self) -> bool {
        true
    }

    fn read(&self, parent: &str) -> Option<String> {
        windows_registry::read_default(&format!(r"{parent}\{HOST_NAME}"))
    }

    fn write(&self, parent: &str, manifest: &Path) -> Result<(), String> {
        let value = manifest.to_string_lossy();
        windows_registry::write_default(&format!(r"{parent}\{HOST_NAME}"), &value)
    }

    fn delete(&self, parent: &str) -> Result<(), String> {
        windows_registry::delete(&format!(r"{parent}\{HOST_NAME}"))
    }
}

#[cfg(windows)]
pub(crate) mod windows_registry {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegGetValueW, RegSetValueExW,
    };
    use windows::core::{PCWSTR, w};

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(crate) fn read_default(key: &str) -> Option<String> {
        let key = wide(key);
        let mut bytes = 0u32;
        let probe = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR::null(),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut bytes),
            )
        };
        if probe != ERROR_SUCCESS || bytes < 2 {
            return None;
        }
        let mut buffer = vec![0u16; bytes as usize / 2];
        let read = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR::null(),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if read != ERROR_SUCCESS {
            return None;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    pub(crate) fn write_default(key: &str, value: &str) -> Result<(), String> {
        let key = wide(key);
        let mut handle = HKEY::default();
        let created = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                w!(""),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut handle,
                None,
            )
        };
        if created != ERROR_SUCCESS {
            return Err(format!("could not create the registry key ({})", created.0));
        }
        let data: Vec<u8> = wide(value).iter().flat_map(|c| c.to_le_bytes()).collect();
        let set = unsafe { RegSetValueExW(handle, PCWSTR::null(), None, REG_SZ, Some(&data)) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if set != ERROR_SUCCESS {
            return Err(format!("could not write the registry value ({})", set.0));
        }
        Ok(())
    }

    /// Deletes a leaf key; a key that is already gone is fine.
    pub(crate) fn delete(key: &str) -> Result<(), String> {
        let key = wide(key);
        let deleted = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr())) };
        if deleted == ERROR_SUCCESS || deleted == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("could not delete the registry key ({})", deleted.0))
        }
    }
}
