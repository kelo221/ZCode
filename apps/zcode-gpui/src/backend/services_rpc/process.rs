//! Process/pipe ownership; all pumping and reaping runs on background threads.
use super::client::{ServiceExitObserver, ServiceSpawnError, StartupTimeouts};
use super::frame::read_frame;
use super::message::decode_message;
use super::process_tree::{ProcessTree, verified_root_and_tree};
use super::state::Shared;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const MAX_HELLO_BYTES: usize = 8 * 1024;

struct ChildGuard {
    child: Child,
    tree: ProcessTree,
    shared: Arc<Shared>,
    completed: bool,
}
impl ChildGuard {
    fn complete(
        &mut self,
        root: std::io::Result<std::process::ExitStatus>,
        tree: Result<(), String>,
    ) {
        self.shared
            .exit
            .complete(verified_root_and_tree(root, tree));
        self.completed = true;
    }
    fn terminate(&mut self) -> Result<(), String> {
        let result = self.tree.terminate();
        let _ = self.child.kill();
        result
    }
    fn reap(mut self, kill: mpsc::Receiver<()>) {
        let termination = loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break Ok(()),
                Ok(None) => {}
                Err(error) => {
                    let _ = self.terminate();
                    let _ = self.child.wait();
                    let tree = self.tree.finish();
                    self.complete(Err(error), tree);
                    return;
                }
            }
            match kill.recv_timeout(Duration::from_millis(20)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break self.terminate(),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        };
        let root = self.child.wait();
        let tree = self.tree.finish();
        // wait 不抢占 stdout EOF：先排空已到达成功回复，再由 reader 拒绝剩余 pending。
        self.complete(root, termination.and(tree));
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.completed {
            // reaper/pipe 建立失败也必须保留退出核验，不能随启动 Err 丢掉偏好写入闸门。
            let termination = self.terminate();
            let root = self.child.wait();
            let tree = self.tree.finish();
            self.complete(root, termination.and(tree));
        }
    }
}

pub(super) fn spawn(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: &Path,
    timeouts: StartupTimeouts,
) -> Result<(Arc<Shared>, u32), ServiceSpawnError> {
    spawn_inner(program, args, envs, cwd, timeouts, None)
}

#[cfg(test)]
pub(super) fn spawn_failing_setup(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: &Path,
    timeouts: StartupTimeouts,
    failure: &'static str,
) -> Result<(Arc<Shared>, u32), ServiceSpawnError> {
    spawn_inner(program, args, envs, cwd, timeouts, Some(failure))
}

fn spawn_inner(
    program: &Path,
    args: &[String],
    envs: &[(String, String)],
    cwd: &Path,
    timeouts: StartupTimeouts,
    failure: Option<&str>,
) -> Result<(Arc<Shared>, u32), ServiceSpawnError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let tree = ProcessTree::prepare(&mut command).map_err(ServiceSpawnError::before_spawn)?;
    let child = command.spawn().map_err(|_| {
        ServiceSpawnError::before_spawn("Services RPC process launch failed".into())
    })?;
    let pid = child.id();
    let (kill_tx, kill_rx) = mpsc::channel::<()>();
    let shared = Arc::new(Shared::new(Some(Box::new(move || {
        let _ = kill_tx.send(());
    }))));
    let fail = |message: &str| {
        shared.close(message);
        ServiceSpawnError {
            message: message.into(),
            exit_observer: Some(ServiceExitObserver::new(shared.clone())),
        }
    };
    let check = |stage: &str| {
        if failure == Some(stage) {
            Err(fail(&format!("Services RPC {stage} setup unavailable")))
        } else {
            Ok(())
        }
    };
    let mut child = ChildGuard {
        child,
        tree,
        shared: shared.clone(),
        completed: false,
    };
    child
        .tree
        .attach(&child.child)
        .map_err(|error| fail(&error))?;
    check("stdin")?;
    let stdin = child
        .child
        .stdin
        .take()
        .ok_or_else(|| fail("Services RPC missing stdin"))?;
    check("stdout")?;
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| fail("Services RPC missing stdout"))?;
    check("stderr")?;
    let mut stderr = child
        .child
        .stderr
        .take()
        .ok_or_else(|| fail("Services RPC missing stderr"))?;
    check("reaper")?;
    std::thread::Builder::new()
        .name("services-rpc-reaper".into())
        .spawn(move || child.reap(kill_rx))
        .map_err(|_| fail("Services RPC reaper unavailable"))?;

    check("writer")?;
    let writer_owner = shared.clone();
    std::thread::Builder::new()
        .name("services-rpc-writer".into())
        .spawn(move || {
            let mut stdin = stdin;
            while let Some(bytes) = writer_owner.take_frame() {
                if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                    writer_owner.close("Services RPC stdin write failed");
                    break;
                }
            }
        })
        .map_err(|_| fail("Services RPC writer unavailable"))?;
    check("stderr pump")?;
    std::thread::Builder::new()
        .name("services-rpc-stderr".into())
        .spawn(move || {
            let mut bytes = [0; 4096];
            loop {
                match stderr.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
        .map_err(|_| fail("Services RPC stderr pump unavailable"))?;
    check("reader")?;
    let (startup_tx, startup_rx) = mpsc::channel();
    let reader_owner = shared.clone();
    std::thread::Builder::new()
        .name("services-rpc-reader".into())
        .spawn(move || {
            let result = read_stdout(BufReader::new(stdout), &reader_owner, &startup_tx);
            let reason = result
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("Services RPC stdout EOF");
            // 读取失败必须直接唤醒启动等待者；sender 尚存活时 close 不会解除 recv_timeout。
            let _ = startup_tx.send(Err(reason.to_owned()));
            reader_owner.close(reason);
        })
        .map_err(|_| fail("Services RPC reader unavailable"))?;
    for (phase, timeout) in [
        ("hello", timeouts.hello),
        ("Initialize", timeouts.initialize),
    ] {
        match startup_rx.recv_timeout(timeout) {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                return Err(fail(&format!(
                    "Services RPC {phase} startup failed: {reason}"
                )));
            }
            Err(_) => {
                return Err(fail(&format!(
                    "Services RPC {phase} startup failed or timed out"
                )));
            }
        }
    }
    if !shared.lock().alive {
        return Err(fail("Services RPC closed during startup"));
    }
    Ok((shared, pid))
}

fn read_stdout(
    mut reader: impl BufRead,
    shared: &Shared,
    startup: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|_| "Services RPC hello read failed")?;
        if available.is_empty() {
            return Err("Services RPC EOF before hello".into());
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |i| i + 1);
        if bytes.len().saturating_add(count) > MAX_HELLO_BYTES {
            return Err("Services RPC hello limit exceeded".into());
        }
        let done = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if done {
            break;
        }
    }
    let hello: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Services RPC invalid hello JSON")?;
    if hello.get("type").and_then(Value::as_str) != Some("zcode-hello")
        || ["version", "platform", "arch"]
            .iter()
            .any(|key| !hello.get(*key).is_some_and(Value::is_string))
        || !hello.get("pid").is_some_and(|pid| pid.as_i64().is_some())
    {
        return Err("Services RPC invalid hello fields".into());
    }
    let ack = json!({"type":"zcode-hello-ack", "version":env!("CARGO_PKG_VERSION"),
        "clientId":format!("gpui-services-{}", uuid::Uuid::now_v7())});
    let mut ack = serde_json::to_vec(&ack).map_err(|_| "Services RPC hello-ack encoding failed")?;
    ack.push(b'\n');
    shared.enqueue(ack)?;
    let _ = startup.send(Ok(()));
    while let Some(frame) = read_frame(&mut reader)? {
        if frame.kind != 1 {
            continue;
        }
        if shared.response(decode_message(&frame.payload)?)? {
            let _ = startup.send(Ok(()));
        }
    }
    Ok(())
}
