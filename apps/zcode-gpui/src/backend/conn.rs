//! Connection lifecycle and IO pumping for the backend child process.

use crate::backend::launcher::BackendLaunch;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

pub enum ConnEvent {
    /// One protocol line from child stdout.
    Line(String),
    /// Diagnostics from child stderr (never parsed as protocol).
    Log(String),
    /// stdout EOF → child is gone.
    Exited,
}

pub struct Connection {
    pub events: futures::channel::mpsc::UnboundedReceiver<ConnEvent>,
    /// Protocol lines to write to child stdin (LF-terminated).
    pub inbound: Sender<String>,
    /// Force-kills the child (and its tree on Windows). Called on idle unload;
    /// on Windows the job object also guarantees death on frontend crash.
    pub kill: Box<dyn FnOnce() + Send>,
}

/// Owned job object handle. Dropping it closes the handle, which (with
/// `KILL_ON_JOB_CLOSE`) also kills any process still in the job; the OS does
/// the same on frontend crash, so agents cannot outlive the app.
#[cfg(windows)]
struct JobHandle(windows::Win32::Foundation::HANDLE);

// SAFETY: a job object handle is a process-wide kernel handle with no thread
// affinity; it is only used for TerminateJobObject/CloseHandle.
#[cfg(windows)]
unsafe impl Send for JobHandle {}

#[cfg(windows)]
impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// Put `child` in a kill-on-close job. `None` on restricted/virtualized
/// Windows setups (nested jobs denied, sandboxed tokens, ...): the caller
/// falls back to tree-killing by pid. Never panics.
#[cfg(windows)]
fn attach_job(child: &Child) -> Option<JobHandle> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    // The handle is owned from here on, so every failure path closes it.
    let job = JobHandle(unsafe { CreateJobObjectW(None, None) }.ok()?);
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .ok()?;
        AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle() as _)).ok()?;
    }
    Some(job)
}

#[cfg(windows)]
fn make_kill(child: &Child) -> Box<dyn FnOnce() + Send> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let pid = child.id();
    let job = attach_job(child);
    if job.is_none() {
        eprintln!(
            "[zcode-gpui] warning: no job object for child pid {pid}; falling back to taskkill"
        );
    }
    Box::new(move || match job {
        Some(job) => unsafe {
            let _ = windows::Win32::System::JobObjects::TerminateJobObject(job.0, 0);
            // `job` drops here and closes the handle.
        },
        None => {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    })
}

#[cfg(not(windows))]
fn make_kill(child: &std::sync::Arc<std::sync::Mutex<Option<Child>>>) -> Box<dyn FnOnce() + Send> {
    let child = child.clone();
    Box::new(move || {
        if let Ok(mut guard) = child.lock() {
            if let Some(c) = guard.as_mut() {
                let _ = c.kill();
            }
        }
    })
}

pub fn spawn_connection(launch: &BackendLaunch, workspace: &Path) -> std::io::Result<Connection> {
    let mut child = Command::new(&launch.program)
        .args(&launch.args)
        .envs(launch.envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let missing = |what: &str| std::io::Error::other(format!("child {what} was not piped"));
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        return Err(missing("stdio"));
    };

    let (in_tx, in_rx) = std::sync::mpsc::channel::<String>();
    let (ev_tx, ev_rx) = futures::channel::mpsc::unbounded::<ConnEvent>();

    std::thread::spawn(move || {
        for line in in_rx {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                break;
            }
        }
    });

    let out_tx = ev_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    let l = l.trim_end_matches('\r');
                    if l.is_empty() {
                        continue;
                    }
                    if out_tx
                        .unbounded_send(ConnEvent::Line(l.to_string()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = out_tx.unbounded_send(ConnEvent::Exited);
    });

    let err_tx = ev_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if err_tx.unbounded_send(ConnEvent::Log(l)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    #[cfg(windows)]
    let kill = make_kill(&child);
    #[cfg(windows)]
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    #[cfg(not(windows))]
    let child = std::sync::Arc::new(std::sync::Mutex::new(Some(child)));
    #[cfg(not(windows))]
    let kill = make_kill(&child);
    #[cfg(not(windows))]
    {
        let watcher_child = child.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let Ok(mut guard) = watcher_child.lock() else {
                    break;
                };
                match guard.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(Some(_))) | Some(Err(_)) | None => break,
                    Some(Ok(None)) => {}
                }
            }
        });
    }

    Ok(Connection {
        events: ev_rx,
        inbound: in_tx,
        kill,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// The kill closure must end the child (job object or taskkill fallback)
    /// and never panic, including when no job could be attached.
    #[test]
    fn kill_terminates_child_tree() {
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let kill = make_kill(&child);
        kill();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "child survived kill");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[test]
    fn dropping_unused_kill_closure_does_not_panic() {
        let mut child = Command::new("cmd").args(["/c", "exit 0"]).spawn().unwrap();
        let _ = child.wait();
        drop(make_kill(&child));
    }
}
