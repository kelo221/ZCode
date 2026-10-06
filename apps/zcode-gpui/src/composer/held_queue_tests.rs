use super::*;
use crate::app::store::AppState;
use crate::conversation::{model::ConversationState, queue::QueueState};
use gpui::{AppContext, TestAppContext};
use serde_json::json;

fn setup(s: &mut AppState, cx: &mut Context<AppState>) -> std::sync::mpsc::Receiver<String> {
    let key = s.workspaces[0].key.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    s.workspaces[0].started = true;
    s.workspaces[0].pumping = true;
    s.workspaces[0].inbound = Some(tx);
    s.active_workspace = Some(key);
    s.active = Some("session".into());
    s.draft = false;
    let mut conv = ConversationState::default();
    conv.apply_snapshot(&json!({"revision":1,"control":{"canStop":false},"inputRouting":{"mode":"choice"},"config":{"followupMode":"queue"},"queue":{"autoDrain":false,"items":[{"queueItemId":"old","text":"held"}]}}));
    s.conversations.insert("session".into(), conv);
    s.composer.update(cx, |c, _| c.set_text("new input"));
    rx
}

#[gpui::test]
fn held_queue_choices_send_reviewed_ids_without_optimistic_mutation(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let rx = setup(s, cx);
        let attached = AttachmentRef {
            reference: "image".into(),
            file_name: "image.png".into(),
            mime: "image/png".into(),
            bytes: 1,
            preview_ref: None,
        };
        for (disposition, expected) in [
            (HeldDisposition::Clear, "clearQueueAndSend"),
            (HeldDisposition::Keep, "keepQueueAndSend"),
        ] {
            s.composer.update(cx, |c, _| {
                c.set_text("new input");
                c.attachments = vec![attached.clone()];
            });
            s.submit_composer_with_trigger(SubmitTrigger::ModifiedEnter, cx);
            let receipt = s.held_confirmation.clone().unwrap();
            assert_eq!(s.composer.read(cx).text(), "new input");
            assert_eq!(s.composer.read(cx).attachments, vec![attached.clone()]);
            assert!(rx.try_recv().is_err());
            s.confirm_held_submission(receipt.clone(), disposition, cx);
            let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
            assert_eq!(command["params"]["type"], "sendText");
            assert_eq!(
                command["params"]["payload"]["heldQueueDisposition"],
                expected
            );
            assert_eq!(
                command["params"]["payload"]["expectedHeldQueueItemIds"],
                json!(["old"])
            );
            assert_eq!(
                command["params"]["payload"]["requestedDelivery"],
                "startNow"
            );
            assert_eq!(
                command["params"]["payload"]["attachments"],
                json!([attached.clone()])
            );
            assert_eq!(
                s.conversations["session"]
                    .queue
                    .as_ref()
                    .unwrap()
                    .items
                    .len(),
                1
            );
            s.confirm_held_submission(receipt, disposition, cx);
            assert!(rx.try_recv().is_err());
            let key = s.active_ws_key().unwrap();
            s.handle_response(
                &key,
                command["id"].as_u64().unwrap(),
                Some(json!({"status":"accepted"})),
                None,
                cx,
            );
            assert!(s.held_confirmation.is_none());
            assert_eq!(
                s.conversations["session"]
                    .queue
                    .as_ref()
                    .unwrap()
                    .items
                    .len(),
                1
            );
        }
    });
}

#[gpui::test]
fn held_queue_cancel_and_binding_changes_never_send(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let rx = setup(s, cx);
        s.begin_held_confirmation(SubmitTrigger::Ordinary, cx);
        let receipt = s.held_confirmation.clone().unwrap();
        s.cancel_held_confirmation(&receipt, cx);
        s.confirm_held_submission(receipt, HeldDisposition::Clear, cx);
        assert_eq!(s.composer.read(cx).text(), "new input");
        for change in [
            "draft",
            "attachments",
            "config",
            "control",
            "connection",
            "navigation",
            "owner",
            "child",
        ] {
            let rx = setup(s, cx);
            s.begin_held_confirmation(SubmitTrigger::Ordinary, cx);
            let receipt = s.held_confirmation.clone().unwrap();
            match change {
                "draft" => s.composer.update(cx, |c, _| c.set_text("newer")),
                "attachments" => s.composer.update(cx, |c, _| {
                    c.attachments.push(AttachmentRef {
                        reference: "new".into(),
                        file_name: "new".into(),
                        mime: "text/plain".into(),
                        bytes: 1,
                        preview_ref: None,
                    })
                }),
                "config" => {
                    s.conversations
                        .get_mut("session")
                        .unwrap()
                        .config
                        .followup_mode = "guide".into()
                }
                "control" => s.conversations.get_mut("session").unwrap().can_stop = true,
                "connection" => s.workspaces[0].generation += 1,
                "navigation" => s.navigation_generation += 1,
                "owner" => s.active_workspace = None,
                "child" => s.viewing_child = Some("child".into()),
                _ => unreachable!(),
            }
            s.confirm_held_submission(receipt, HeldDisposition::Clear, cx);
            assert!(s.held_confirmation.is_none());
            assert!(s.workspaces[0].pending.is_empty());
            assert!(rx.try_recv().is_err());
        }
        assert!(rx.try_recv().is_err());
    });
}

#[gpui::test]
fn held_queue_changed_ids_and_stale_ack_require_new_choice(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let rx = setup(s, cx);
        s.begin_held_confirmation(SubmitTrigger::Ordinary, cx);
        let receipt = s.held_confirmation.clone().unwrap();
        s.conversations.get_mut("session").unwrap().queue = QueueState::from_value(
            &json!({"autoDrain":false,"items":[{"queueItemId":"new","text":"new held"}]}),
        );
        s.confirm_held_submission(receipt, HeldDisposition::Clear, cx);
        assert!(rx.try_recv().is_err());
        let receipt = s.held_confirmation.clone().unwrap();
        assert_eq!(receipt.queue_ids, vec!["new"]);
        s.confirm_held_submission(receipt, HeldDisposition::Keep, cx);
        let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        s.composer.update(cx, |c, _| c.set_text("newer"));
        let key = s.active_ws_key().unwrap();
        s.handle_response(
            &key,
            command["id"].as_u64().unwrap(),
            Some(json!({"status":"failed","reasonCode":"guard.heldQueueConfirmationStale"})),
            None,
            cx,
        );
        assert_eq!(s.composer.read(cx).text(), "new input\nnewer");
        assert!(s.held_confirmation.is_some());
        assert!(rx.try_recv().is_err());
        assert!(s.workspaces[0].pending.is_empty());
    });
}

#[gpui::test]
fn held_queue_goal_objective_uses_disposition_without_delivery_override(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let rx = setup(s, cx);
        for text in ["/goal resume", "/compact"] {
            s.composer.update(cx, |c, _| c.set_text(text));
            assert!(!s.begin_held_confirmation(SubmitTrigger::ModifiedEnter, cx));
        }
        s.composer
            .update(cx, |c, _| c.set_text("/goal new objective"));
        s.submit_composer_with_trigger(SubmitTrigger::ModifiedEnter, cx);
        let receipt = s.held_confirmation.clone().unwrap();
        assert!(rx.try_recv().is_err());
        s.confirm_held_submission(receipt, HeldDisposition::Keep, cx);
        let command: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(command["params"]["type"], "sendGoalCommand");
        assert_eq!(command["params"]["payload"]["text"], "new objective");
        assert_eq!(
            command["params"]["payload"]["heldQueueDisposition"],
            "keepQueueAndSend"
        );
        assert_eq!(
            command["params"]["payload"]["expectedHeldQueueItemIds"],
            json!(["old"])
        );
        assert!(
            command["params"]["payload"]
                .get("requestedDelivery")
                .is_none()
        );
        let key = s.active_ws_key().unwrap();
        s.handle_response(
            &key,
            command["id"].as_u64().unwrap(),
            Some(json!({"status":"failed","reasonCode":"guard.heldQueueConfirmationStale"})),
            None,
            cx,
        );
        assert_eq!(s.composer.read(cx).text(), "/goal new objective");
        assert!(s.held_confirmation.is_some());
        assert!(rx.try_recv().is_err());
    });
}

#[gpui::test]
fn held_queue_failed_enqueue_keeps_owned_attachments(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let rx = setup(s, cx);
        let owned = std::env::temp_dir().join(uuid::Uuid::now_v7().to_string());
        s.composer
            .update(cx, |c, _| c.temp_owned.push(owned.clone()));
        s.begin_held_confirmation(SubmitTrigger::Ordinary, cx);
        let receipt = s.held_confirmation.clone().unwrap();
        drop(rx);
        s.confirm_held_submission(receipt, HeldDisposition::Keep, cx);
        assert_eq!(s.composer.read(cx).text(), "new input");
        assert_eq!(s.composer.read(cx).temp_owned, vec![owned]);
        assert!(s.retired_temp_files.is_empty());
        assert!(s.workspaces[0].pending.is_empty());
    });
}
