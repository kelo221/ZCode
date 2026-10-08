//! Real synthetic descendants use this test executable, never a Host/user root.
use super::{ServiceValue, client::StartupTimeouts, fake_child, process, process_tree::*};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const FIXTURE: &str = "backend::services_rpc::process_tree_tests::owned_tree_fixture";

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("zcode-tree-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture_args() -> Vec<String> {
    ["--ignored", "--exact", FIXTURE, "--nocapture", "--quiet"]
        .map(String::from)
        .to_vec()
}
fn fixture_env(mode: &str, root: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("ZCODE_TREE_FIXTURE".into(), mode.into()),
        (
            "ZCODE_TREE_SCRATCH".into(),
            root.to_string_lossy().into_owned(),
        ),
    ];
    if let Ok(value) = std::env::var("SystemRoot") {
        env.push(("SystemRoot".into(), value));
    }
    env
}
fn timeouts() -> StartupTimeouts {
    StartupTimeouts {
        hello: Duration::from_secs(3),
        initialize: Duration::from_secs(3),
    }
}

#[test]
#[ignore = "child process fixture; run only via --exact and explicit synthetic env"]
fn owned_tree_fixture() {
    let Ok(mode) = std::env::var("ZCODE_TREE_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("ZCODE_TREE_SCRATCH").unwrap());
    if mode == "leaf" {
        std::fs::write(root.join("leaf-heartbeat"), b"alive").unwrap();
        std::fs::write(root.join("leaf-ready"), b"ready").unwrap();
        loop {
            std::fs::write(root.join("leaf-heartbeat"), b"alive").unwrap();
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    loop {
        std::thread::park();
    }
}

fn heartbeat_stopped(root: &Path) {
    let file = root.join("leaf-heartbeat");
    let before = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(std::fs::metadata(file).unwrap().modified().unwrap(), before);
}

#[test]
fn root_exit_drains_success_and_verifies_real_descendant_cleanup() {
    let scratch = Scratch::new();
    let client = fake_child::spawn_tree("normal", &scratch.0).unwrap();
    let observer = client.exit_observer();
    assert!(observer.wait_for_exit(Duration::ZERO).is_err());
    let call = client.call("test", "reply_exit", vec![]).unwrap();
    assert_eq!(
        futures::executor::block_on(call.receiver).unwrap().unwrap(),
        ServiceValue::Json(serde_json::json!("drained"))
    );
    observer.wait_for_exit(Duration::from_secs(6)).unwrap();
    heartbeat_stopped(&scratch.0);
    drop(client);
    observer.wait_for_exit(Duration::ZERO).unwrap();
}

#[test]
fn hello_initialize_failures_retain_real_tree_observer_and_signal_promptly() {
    for mode in [
        "bad_hello",
        "bad_initialize",
        "truncated",
        "silent",
        "no_initialize",
    ] {
        let scratch = Scratch::new();
        let start = Instant::now();
        let error = match fake_child::spawn_tree(mode, &scratch.0) {
            Ok(_) => panic!("accepted {mode}"),
            Err(error) => error,
        };
        if mode.starts_with("bad_") || mode == "truncated" {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "reader error waited for watchdog"
            );
        }
        error
            .exit_observer
            .expect("post-spawn error lost observer")
            .wait_for_exit(Duration::from_secs(6))
            .unwrap();
        heartbeat_stopped(&scratch.0);
    }
}

#[test]
fn every_early_pipe_and_thread_setup_failure_retains_exit_proof() {
    let scratch = Scratch::new();
    let program = std::env::current_exe().unwrap();
    for stage in [
        "stdin",
        "stdout",
        "stderr",
        "reaper",
        "writer",
        "stderr pump",
        "reader",
    ] {
        let result = process::spawn_failing_setup(
            &program,
            &fixture_args(),
            &fixture_env("before-hello", &scratch.0),
            &scratch.0,
            timeouts(),
            stage,
        );
        let error = match result {
            Ok(_) => panic!("setup did not fail"),
            Err(error) => error,
        };
        assert!(error.message.contains(stage));
        error
            .exit_observer
            .expect("early setup error lost observer")
            .wait_for_exit(Duration::from_secs(6))
            .unwrap();
    }
}

#[test]
fn tree_probe_errors_and_root_wait_errors_never_prove_exit() {
    assert!(verify_empty(|| Err("synthetic query failure".into())).is_err());
    assert!(verify_empty(|| Ok(false)).is_ok());
    let root = Err(std::io::Error::other("synthetic wait error"));
    assert!(
        verified_root_and_tree(root, Ok(()))
            .unwrap_err()
            .contains("root wait failed")
    );
}
