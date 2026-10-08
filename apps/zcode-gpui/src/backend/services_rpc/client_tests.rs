use super::{ServiceClient, ServiceValue, fake_child, state::*};
use serde_json::json;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn receive(call: super::ServiceCall) -> Result<ServiceValue, String> {
    futures::executor::block_on(call.receiver).expect("oneshot must be settled explicitly")
}

#[test]
fn client_hello_initializes_then_correlates_out_of_order() {
    let mut client = fake_child::spawn("normal").unwrap();
    assert!(client.is_alive());
    assert!(client.pid() > 0);
    let first = client.call("test", "pair", vec![json!("first")]).unwrap();
    let second = client.call("test", "pair", vec![json!("second")]).unwrap();
    assert!(second.id > first.id);
    assert_eq!(
        receive(second).unwrap(),
        ServiceValue::Json(json!("second"))
    );
    assert_eq!(receive(first).unwrap(), ServiceValue::Json(json!("first")));
    assert_eq!(client.pending_count(), 0);
    assert_eq!(
        receive(client.call("test", "void", vec![]).unwrap()).unwrap(),
        ServiceValue::Undefined
    );
    assert_eq!(
        receive(client.call("test", "null", vec![]).unwrap()).unwrap(),
        ServiceValue::Json(json!(null))
    );
    client.shutdown();
    assert!(!client.is_alive());
    assert!(client.call("test", "void", vec![]).is_err());
    client.shutdown();
}

#[test]
fn eof_settles_all_and_remote_errors_do_not_dump_payload() {
    let client = fake_child::spawn("normal").unwrap();
    for method in ["error", "error_obj"] {
        let error = receive(client.call("test", method, vec![]).unwrap()).unwrap_err();
        assert!(!error.contains("synthetic-secret"));
    }
    let waiting = client.call("test", "hold", vec![]).unwrap();
    let exiting = client.call("test", "exit", vec![]).unwrap();
    assert!(receive(waiting).is_err());
    assert!(receive(exiting).is_err());
    assert!(!client.is_alive());
    assert_eq!(client.pending_count(), 0);
}

#[test]
fn cancellation_is_advisory_idempotent_and_late_replies_are_ignored() {
    let client = fake_child::spawn("normal").unwrap();
    let call = client.call("test", "hold", vec![]).unwrap();
    assert!(call.cancel().unwrap());
    assert!(!call.cancel().unwrap());
    assert!(receive(call).unwrap_err().contains("rollback"));
    let next = client
        .call("test", "echo", vec![json!({"ok":true})])
        .unwrap();
    assert_eq!(
        receive(next).unwrap(),
        ServiceValue::Json(json!({"ok":true}))
    );
    assert_eq!(client.pending_count(), 0);
}

#[test]
fn dropped_call_does_not_cancel_a_committed_mutation() {
    let client = fake_child::spawn("normal").unwrap();
    drop(client.call("test", "hold", vec![]).unwrap());
    let count = receive(client.call("test", "cancel_count", vec![]).unwrap()).unwrap();
    assert_eq!(count, ServiceValue::Json(json!(0)));
    assert_eq!(client.pending_count(), 1);
}

#[test]
fn pending_bound_rejects_before_send_and_drop_settles_all() {
    let client = fake_child::spawn("normal").unwrap();
    let mut calls = Vec::new();
    for _ in 0..MAX_PENDING_CALLS {
        calls.push(client.call("test", "hold", vec![]).unwrap());
    }
    assert_eq!(client.pending_count(), MAX_PENDING_CALLS);
    assert!(client.call("test", "hold", vec![]).is_err());
    drop(client);
    for call in calls {
        assert!(receive(call).is_err());
    }
}

#[test]
fn concurrent_call_admission_keeps_fifo_and_unique_ids() {
    let client = Arc::new(fake_child::spawn("normal").unwrap());
    let barrier = Arc::new(Barrier::new(9));
    let mut threads = Vec::new();
    for i in 0..8 {
        let (client, barrier) = (client.clone(), barrier.clone());
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            let call = client.call("test", "echo", vec![json!(i)]).unwrap();
            let id = call.id;
            assert_eq!(receive(call).unwrap(), ServiceValue::Json(json!(i)));
            id
        }));
    }
    barrier.wait();
    let mut ids: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 8);
}

#[test]
fn malformed_startup_and_protocol_fail_closed() {
    for mode in ["bad_hello", "bad_initialize", "truncated"] {
        assert!(fake_child::spawn(mode).is_err(), "accepted {mode}");
    }
    for method in ["oversize", "future_id", "bad_frame"] {
        let client = fake_child::spawn("normal").unwrap();
        assert!(receive(client.call("test", method, vec![]).unwrap()).is_err());
        assert!(!client.is_alive());
    }
}

#[test]
fn startup_watchdogs_are_bounded_and_kill_the_child() {
    for mode in ["silent", "no_initialize"] {
        let start = Instant::now();
        assert!(fake_child::spawn_with_timeouts(mode, Duration::from_millis(150)).is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return false;
    };
    let alive = unsafe { WaitForSingleObject(handle, 0).0 == 258 };
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(handle);
    }
    alive
}

#[cfg(windows)]
#[test]
fn drop_kills_owned_child_without_joining_on_caller() {
    let client = fake_child::spawn("normal").unwrap();
    let pid = client.pid();
    let start = Instant::now();
    drop(client);
    assert!(start.elapsed() < Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(pid) {
        assert!(Instant::now() < deadline, "owned process survived Drop");
        std::thread::yield_now();
    }
}

#[test]
fn queue_bytes_and_frames_are_bounded_without_waiting_for_io() {
    let owner = Shared::new(None);
    let mut state = owner.lock();
    for _ in 0..MAX_QUEUED_FRAMES {
        state.enqueue(vec![0]).unwrap();
    }
    assert!(state.enqueue(vec![0]).is_err());
    state.queue.clear();
    state.queued_bytes = MAX_QUEUED_BYTES;
    assert!(state.enqueue(vec![0]).is_err());
    drop(state);
    owner.close("test closed");
}

#[test]
fn cancellation_queue_failure_preserves_pending_and_close_is_idempotent() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let killed = Arc::new(AtomicUsize::new(0));
    let kill_count = killed.clone();
    let owner = Shared::new(Some(Box::new(move || {
        kill_count.fetch_add(1, Ordering::SeqCst);
    })));
    assert!(owner.response(super::message::Message::Initialize).unwrap());
    let (id, receiver) = owner.admit("test", "hold", vec![]).unwrap();
    {
        let mut state = owner.lock();
        state.queued_bytes = MAX_QUEUED_BYTES;
    }
    assert!(owner.cancel(id).is_err());
    assert_eq!(owner.lock().pending.len(), 1);
    owner.close("closed");
    owner.close("closed twice");
    assert_eq!(killed.load(Ordering::SeqCst), 1);
    assert_eq!(
        futures::executor::block_on(receiver).unwrap(),
        Err("closed".into())
    );
}

#[test]
fn id_exhaustion_and_terminal_response_do_not_reuse_or_resurrect_calls() {
    let owner = Shared::new(None);
    owner.response(super::message::Message::Initialize).unwrap();
    owner.lock().next_id = i32::MAX as u32;
    let (id, receiver) = owner.admit("test", "hold", vec![]).unwrap();
    assert_eq!(id, i32::MAX as u32);
    assert!(owner.admit("test", "hold", vec![]).is_err());
    owner.close("closed");
    assert!(
        !owner
            .response(super::message::Message::Success(
                id,
                ServiceValue::Undefined
            ))
            .unwrap()
    );
    assert_eq!(
        futures::executor::block_on(receiver).unwrap(),
        Err("closed".into())
    );
    assert_eq!(owner.lock().pending.len(), 0);
}

#[test]
fn call_wait_handles_reply_and_deadline_without_claiming_rollback() {
    let client = fake_child::spawn("normal").unwrap();
    let call = client.call("test", "echo", vec![json!("read")]).unwrap();
    let value = futures::executor::block_on(call.wait(futures::future::pending::<()>())).unwrap();
    assert_eq!(value, ServiceValue::Json(json!("read")));
    let call = client.call("test", "hold", vec![]).unwrap();
    let error = futures::executor::block_on(call.wait(futures::future::ready(()))).unwrap_err();
    assert!(error.contains("timed out"));
    assert!(error.contains("unknown"));
    assert_eq!(client.pending_count(), 0);
    let count = receive(client.call("test", "cancel_count", vec![]).unwrap()).unwrap();
    assert_eq!(count, ServiceValue::Json(json!(1)));
}

#[test]
fn sanitized_spawn_failure_does_not_include_caller_paths() {
    let path = std::path::Path::new("synthetic-secret-program-not-present");
    let error = match ServiceClient::spawn(path, &[], &[], std::path::Path::new(".")) {
        Ok(_) => panic!("unexpected child"),
        Err(error) => error,
    };
    assert!(!error.contains("synthetic-secret"));
    let error = match ServiceClient::spawn_observed(path, &[], &[], std::path::Path::new(".")) {
        Ok(_) => panic!("unexpected child"),
        Err(error) => error,
    };
    assert!(!error.message.contains("synthetic-secret"));
    assert!(
        error.exit_observer.is_none(),
        "pre-spawn failure owns no process"
    );
}
