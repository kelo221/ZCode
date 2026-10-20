//! Subagent cards expose a copyable agent ID so the user can ask the agent to
//! resume that subagent (SendMessage) after a restart or a lost connection.

use super::subagent_card::{agent_id_from_child_session, short_agent_id};
use crate::app::{
    root::RootView,
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::{ConversationState, Row};
use gpui::{TestApp, px, size};
use std::sync::Arc;

const AGENT_ID: &str = "agent_7fbbd55f-28a1-4cf3-bf7b-2d891071e295";

#[test]
fn agent_id_comes_from_the_child_session_name() {
    assert_eq!(
        agent_id_from_child_session(&format!("sess_subagent_{AGENT_ID}")),
        Some(AGENT_ID)
    );
    assert_eq!(agent_id_from_child_session("sess_123"), None);
    assert_eq!(agent_id_from_child_session("sess_subagent_other"), None);
    assert_eq!(short_agent_id(AGENT_ID), "agent_7fbbd55f");
    assert_eq!(short_agent_id("agent_plain"), "agent_plain");
}

#[test]
fn clicking_the_id_chip_copies_the_full_agent_id() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            crate::shared::settings::AppSettings {
                gpui_noise_free: Some(false),
                ..Default::default()
            },
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| true),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    app.update_entity(&state, |s, _| {
        s.active_workspace = Some(s.workspaces[0].key.clone());
        s.active = Some("s".into());
        let mut conv = ConversationState {
            subscribed: true,
            ..Default::default()
        };
        conv.rows.insert(
            1,
            Row::Subagent {
                row_id: 1,
                subagent_type: "Explore".into(),
                status: "failed".into(),
                summary_text: "Lost midway".into(),
                parent_tool_call_id: None,
                child_session_id: Some(format!("sess_subagent_{AGENT_ID}")),
                work_id: None,
                backgrounded: false,
                started_at: None,
            },
        );
        s.conversations.insert("s".into(), conv);
    });
    let mut window = app.open_window(|_, cx| RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    window.draw();

    let chip = app.update(|cx| cx.global::<TestTargets>().0[&format!("agent-id-copy-{AGENT_ID}")]);
    window.simulate_click(chip.center(), gpui::MouseButton::Left);
    assert_eq!(
        app.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some(AGENT_ID)
    );
}
