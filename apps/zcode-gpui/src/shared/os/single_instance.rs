//! Single-instance guard and forward-to-first IPC.
//! On Windows, uses a named mutex (Local\ZCodeGPUI_SingleInstance) to elect the primary
//! and local loopback IPC to forward launch requests from secondary instances.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceMessage {
    pub action: String,
    pub workspace: Option<String>,
}

pub enum InstanceRole {
    Primary(SingleInstanceGuard),
    Secondary,
}

pub struct SingleInstanceGuard {
    #[cfg(windows)]
    _mutex_handle: windows::Win32::Foundation::HANDLE,
    running: Arc<AtomicBool>,
    port_file: PathBuf,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.port_file);
        #[cfg(windows)]
        unsafe {
            use windows::Win32::Foundation::CloseHandle;
            if !self._mutex_handle.is_invalid() {
                let _ = CloseHandle(self._mutex_handle);
            }
        }
    }
}

pub fn port_file_path() -> PathBuf {
    let home = std::env::var("ZCODE_DESKTOP_HOME_DIR")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    PathBuf::from(home)
        .join(".zcode")
        .join("v2")
        .join("gpui_instance.port")
}

#[cfg(windows)]
fn acquire_windows_mutex(name: &str) -> (windows::Win32::Foundation::HANDLE, bool) {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::HSTRING;

    let hstring = HSTRING::from(name);
    unsafe {
        let handle = CreateMutexW(None, true, &hstring).unwrap_or_default();
        let already_exists = GetLastError() == ERROR_ALREADY_EXISTS;
        (handle, already_exists)
    }
}

pub fn try_acquire_single_instance(
    mutex_name: &str,
    launch_msg: InstanceMessage,
    on_message: impl Fn(InstanceMessage) + Send + Sync + 'static,
) -> InstanceRole {
    let port_file = port_file_path();

    #[cfg(windows)]
    {
        let (handle, already_exists) = acquire_windows_mutex(mutex_name);
        if already_exists {
            // Secondary instance: forward launch message and exit
            if let Ok(port_str) = std::fs::read_to_string(&port_file)
                && let Ok(port) = port_str.trim().parse::<u16>()
                && let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port))
                && let Ok(json) = serde_json::to_string(&launch_msg)
            {
                let _ = stream.write_all(json.as_bytes());
                let _ = stream.flush();
            }
            unsafe {
                use windows::Win32::Foundation::CloseHandle;
                if !handle.is_invalid() {
                    let _ = CloseHandle(handle);
                }
            }
            return InstanceRole::Secondary;
        }

        // Primary instance
        let listener = match TcpListener::bind("127.0.0.1:0") {
            Ok(l) => l,
            Err(_) => return InstanceRole::Secondary,
        };

        if let Ok(addr) = listener.local_addr() {
            if let Some(parent) = port_file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&port_file, addr.port().to_string());
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        std::thread::spawn(move || {
            let _ = listener.set_nonblocking(false);
            while running_clone.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let mut buf = Vec::new();
                    let mut temp = [0u8; 1024];
                    while let Ok(n) = stream.read(&mut temp) {
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&temp[..n]);
                        if buf.len() > 16384 {
                            break;
                        }
                    }
                    if let Ok(msg) = serde_json::from_slice::<InstanceMessage>(&buf) {
                        on_message(msg);
                    }
                }
            }
        });

        InstanceRole::Primary(SingleInstanceGuard {
            _mutex_handle: handle,
            running,
            port_file,
        })
    }

    #[cfg(not(windows))]
    {
        let _ = mutex_name;
        let _ = launch_msg;
        let _ = on_message;
        InstanceRole::Primary(SingleInstanceGuard {
            running: Arc::new(AtomicBool::new(true)),
            port_file,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serde() {
        let msg = InstanceMessage {
            action: "activate".into(),
            workspace: Some("C:\\path\\to\\project".into()),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: InstanceMessage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.action, "activate");
        assert_eq!(
            deserialized.workspace.as_deref(),
            Some("C:\\path\\to\\project")
        );
    }
}
