//! Backend launcher: resolves and spawns the ZCode agent backend
//! (`app-server --stdio`) and pumps its stdio.
//!
//! The backend is an external process speaking NDJSON on stdin/stdout. This
//! frontend never links backend code, so upstream backend updates flow in
//! unchanged; only the protocol version on the wire matters.
//!
//! Resolution order (first spawnable candidate wins; the caller falls back to
//! the next candidate if a process dies before storage startup completes):
//!   1. `ZCODE_GPUI_AGENT_PROGRAM` + optional `ZCODE_GPUI_AGENT_ARGS` env
//!      override (debug/testing escape hatch).
//!   2. bun + monorepo dev sources (apps/zcode-cli/packages/cli/src/main.ts),
//!      per the user's "no Node runtime" requirement; needs the CLI workspace
//!      deps installed (bun install) and packages built (turbo run build).
//!   3. The installed ZCode desktop app's bundled runtime
//!      (`zcode.cjs` under ELECTRON_RUN_AS_NODE=1), which is the same launch
//!      contract packages/services/src/zcode-agent/zcodeAgentProcessManager.ts
//!      uses for packaged builds. This backend updates with the desktop app.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub struct BackendLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
    pub describe: String,
}

pub fn resolve_candidates(workspace: &Path) -> Vec<BackendLaunch> {
    let mut out = Vec::new();

    if let Ok(program) = std::env::var("ZCODE_GPUI_AGENT_PROGRAM") {
        let args = std::env::var("ZCODE_GPUI_AGENT_ARGS")
            .map(|a| a.split_whitespace().map(str::to_string).collect::<Vec<_>>())
            .unwrap_or_else(|_| vec!["app-server".into(), "--stdio".into()]);
        out.push(BackendLaunch {
            program,
            args,
            envs: vec![],
            describe: "env override".into(),
        });
    }

    for root in repo_root_candidates(workspace) {
        let main = root
            .join("apps")
            .join("zcode-cli")
            .join("packages")
            .join("cli")
            .join("src")
            .join("main.ts");
        if main.exists() {
            out.push(BackendLaunch {
                program: "bun".into(),
                args: vec![
                    main.to_string_lossy().into_owned(),
                    "app-server".into(),
                    "--stdio".into(),
                ],
                envs: vec![],
                describe: "bun + repo source".into(),
            });
            break;
        }
    }

    if let Some(launch) = installed_app_launch() {
        out.push(launch);
    }
    out
}

fn repo_root_candidates(workspace: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |dir: &Path| {
        let mut cur = Some(dir.to_path_buf());
        while let Some(d) = cur {
            roots.push(d.clone());
            cur = d.parent().map(Path::to_path_buf);
        }
    };
    push(workspace);
    if let Ok(cwd) = std::env::current_dir() {
        push(&cwd);
    }
    roots
}

fn installed_app_launch() -> Option<BackendLaunch> {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("LOCALAPPDATA").ok()?;
        let app = PathBuf::from(base)
            .join("Programs")
            .join("ZCode")
            .join("ZCode.exe");
        let cjs = app
            .parent()?
            .join("resources")
            .join("glm")
            .join("zcode.cjs");
        if !app.exists() || !cjs.exists() {
            return None;
        }
        Some(BackendLaunch {
            program: app.to_string_lossy().into_owned(),
            args: vec![
                cjs.to_string_lossy().into_owned(),
                "app-server".into(),
                "--stdio".into(),
            ],
            envs: vec![("ELECTRON_RUN_AS_NODE".into(), "1".into())],
            describe: "installed ZCode app runtime".into(),
        })
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/ZCode.app/Contents/MacOS/ZCode");
        let cjs = PathBuf::from("/Applications/ZCode.app/Contents/Resources/glm/zcode.cjs");
        if !app.exists() || !cjs.exists() {
            return None;
        }
        return Some(BackendLaunch {
            program: app.to_string_lossy().into_owned(),
            args: vec![
                cjs.to_string_lossy().into_owned(),
                "app-server".into(),
                "--stdio".into(),
            ],
            envs: vec![("ELECTRON_RUN_AS_NODE".into(), "1".into())],
            describe: "installed ZCode app runtime".into(),
        });
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        None
    }
}

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

#[cfg(windows)]
fn make_kill(child: &Child) -> Box<dyn FnOnce() + Send> {
    // Job object with KILL_ON_JOB_CLOSE: the kernel kills the whole tree even
    // if this frontend crashes — mirrors the desktop's processTreeOwnership.
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    // SAFETY: creating a kernel object and binding one child handle to it.
    let job = unsafe { CreateJobObjectW(None, None) }.expect("CreateJobObjectW");
    unsafe {
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .expect("SetInformationJobObject");
        AssignProcessToJobObject(job, HANDLE(child.as_raw_handle() as _))
            .expect("AssignProcessToJobObject");
    }
    // HANDLE is a raw pointer, so it isn't Send; carry it as usize (closure
    // field-capture would otherwise grab the raw pointer directly).
    let job = job.0 as usize;
    Box::new(move || unsafe {
        let _ = TerminateJobObject(HANDLE(job as *mut _), 0);
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

    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

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

    // Kill handle + zombie reaper. On Windows the job object covers crash
    // safety; elsewhere a shared handle + polling watcher does both jobs.
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

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_command_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
