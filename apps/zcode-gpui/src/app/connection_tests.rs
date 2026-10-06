use crate::app::store::AppState;
use crate::backend::backlog::{EventBacklog, push_event};
use crate::backend::events::attach_pump;
use crate::backend::launcher::ConnEvent;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn superseded_pump_drops_old_line_log_exit_and_flow_work(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let backlog = EventBacklog::new(1024 * 1024);
    let logs = EventBacklog::new(1024 * 1024);
    let key = cx.read_entity(&state, |s, _| s.workspaces[0].key.clone());
    state.update(cx, |s, cx| {
        attach_pump(cx, key.clone(), 0, backlog.clone(), logs.clone(), rx);
        s.workspaces[0].invalidate_connection();
        s.workspaces[0].started = true;
        s.workspaces[0].pumping = true;
        s.workspaces[0].status = "replacement".into();
    });
    let line = r#"{"method":"startup/storageState","params":{"phase":"ready"}}"#;
    assert!(push_event(
        &backlog,
        &tx,
        ConnEvent::Line(line.into()),
        line.len()
    ));
    assert!(push_event(&logs, &tx, ConnEvent::Log("old-log".into()), 7));
    tx.unbounded_send(ConnEvent::Exited).unwrap();
    drop(tx);
    cx.run_until_parked();
    state.read_with(cx, |s, _| {
        assert_eq!(s.workspaces[0].status, "replacement");
        assert!(s.workspaces[0].pumping);
        assert_eq!(s.workspaces[0].restart_attempts, 0);
        assert!(s.flow_saturated.is_empty());
        assert!(s.log.is_empty());
    });
}

#[gpui::test]
fn scheduled_restart_cannot_respawn_after_unload_generation(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.workspaces[0].started = true;
        s.workspaces[0].pumping = true;
        s.handle_conn(&key, ConnEvent::Exited, cx);
        s.workspaces[0].invalidate_connection();
        s.workspaces[0].status = "idle replacement".into();
    });
    cx.run_until_parked();
    cx.read_entity(&state, |s, _| {
        assert_eq!(s.workspaces[0].status, "idle replacement");
        assert!(!s.workspaces[0].pumping);
    });
}
