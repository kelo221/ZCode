use super::*;
use crate::app::{
    store::AppState,
    test_support::{RUNTIME_TEST_LOCK, TestTargets},
};
use crate::conversation::model::ConversationState;
use gpui::{MouseButton, TestApp, px, size};
use std::sync::Arc;

fn tool(row_id: u64, name: &str, status: &str) -> Row {
    Row::ToolCall {
        row_id,
        label: name.into(),
        tool_name: name.into(),
        status: status.into(),
        input_text: "{}".into(),
        output_text: String::new(),
        tool_call_id: None,
        display: None,
    }
}

fn text(row_id: u64, body: &str) -> Row {
    Row::AssistantText {
        row_id,
        entity_id: format!("t{row_id}"),
        text: body.into(),
        state: "done".into(),
        can_retry: false,
        created_at: None,
    }
}

fn thinking(row_id: u64, body: &str) -> Row {
    Row::Reasoning {
        row_id,
        text: body.into(),
        duration_ms: None,
    }
}

fn places(rows: &[Row]) -> Vec<Place> {
    (0..rows.len())
        .map(|ix| place(ix, rows.len(), |i| kind(&rows[i])))
        .collect()
}

#[test]
fn tool_runs_fold_between_text_and_keep_todos_and_prompts() {
    let rows = [
        text(1, "Looking."),
        tool(2, "Read", "success"),
        thinking(3, ""),
        tool(4, "Bash", "running"),
        tool(5, "mcp__x__Grep", "error"),
        tool(6, "TodoWrite", "success"),
        tool(7, "Edit", "pendingApproval"),
        tool(8, "Edit", "success"),
        text(9, "Done."),
    ];
    assert_eq!(
        places(&rows),
        vec![
            Place::Show,
            Place::Leader(Summary {
                count: 3,
                running: 1,
                failed: 1
            }),
            Place::Show,
            Place::Member { leader: 1 },
            Place::Member { leader: 1 },
            Place::Show,
            Place::Show,
            Place::Leader(Summary {
                count: 1,
                running: 0,
                failed: 0
            }),
            Place::Show,
        ]
    );
}

#[test]
fn thinking_with_text_splits_runs_and_empty_rows_do_not() {
    assert_eq!(kind(&thinking(1, "")), Kind::Transparent);
    assert_eq!(kind(&thinking(1, "plan")), Kind::Keep);
    assert_eq!(kind(&text(1, "  ")), Kind::Transparent);
    let rows = [
        tool(1, "Read", "success"),
        thinking(2, "plan"),
        tool(3, "Read", "success"),
    ];
    assert!(matches!(
        places(&rows)[2],
        Place::Leader(Summary { count: 1, .. })
    ));
}

#[test]
fn a_growing_run_keeps_its_leader() {
    let mut rows = vec![text(1, "Go."), tool(2, "Read", "success")];
    assert!(matches!(
        places(&rows)[1],
        Place::Leader(Summary { count: 1, .. })
    ));
    rows.push(tool(3, "Bash", "running"));
    let now = places(&rows);
    assert!(matches!(
        now[1],
        Place::Leader(Summary {
            count: 2,
            running: 1,
            ..
        })
    ));
    assert_eq!(now[2], Place::Member { leader: 1 });
}

#[test]
fn fold_line_expands_its_run_and_the_toggle_turns_folding_off() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        cx.set_global(TestTargets::default());
        crate::shared::preferences::Preferences::install_at(
            Default::default(),
            std::env::temp_dir().join(uuid::Uuid::now_v7().to_string()),
            Arc::new(|| false),
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
        for row in [
            text(1, "Looking."),
            tool(2, "Read", "success"),
            tool(3, "Bash", "success"),
        ] {
            let id = match &row {
                Row::AssistantText { row_id, .. } | Row::ToolCall { row_id, .. } => *row_id,
                _ => unreachable!(),
            };
            conv.rows.insert(id, row);
        }
        s.conversations.insert("s".into(), conv);
    });
    let mut window = app.open_window(|_, cx| crate::app::root::RootView::new(state.clone(), cx));
    window.simulate_resize(size(px(1280.), px(820.)));
    window.update(|_, window, cx| window.bounds_changed(cx));
    window.draw();
    window.draw();
    let targets = app.read(|cx| cx.global::<TestTargets>().0.clone());
    assert!(
        targets.contains_key("noise-fold-2"),
        "the run folds by default"
    );

    window.simulate_click(targets["noise-fold-2"].center(), MouseButton::Left);
    window.read(|v, _| assert!(v.expanded_folds.contains(&2)));

    let toggle = app.read(|cx| cx.global::<TestTargets>().0["composer-noise-free"].center());
    window.simulate_click(toggle, MouseButton::Left);
    app.read(|cx| {
        let owner = cx
            .global::<crate::shared::preferences::PreferenceOwner>()
            .0
            .read(cx);
        assert_eq!(owner.snapshot.gpui_noise_free, Some(false));
        assert!(!enabled(cx));
    });
}
