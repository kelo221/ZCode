use super::*;
use crate::app::store::AppState;
use gpui::{AppContext, TestAppContext};
use serde_json::json;

pub(crate) fn commands() -> Value {
    json!([
        {"name":"init","description":"Initialize instructions","inputHint":"/init [notes]","source":"builtin"},
        {"name":"review-local","description":"Review changes","source":"custom"}
    ])
}

#[test]
fn slash_catalog_decodes_strict_commands_and_empty_replacement() {
    let parsed = parse_commands(&commands()).unwrap();
    assert_eq!(parsed[0].name, "init");
    assert_eq!(
        parsed[1].source,
        Some(crate::composer::slash::SlashSource::Custom)
    );
    assert!(parse_commands(&json!([])).unwrap().is_empty());
    for invalid in [
        json!([{"name":"init","description":"ok","enabled":true}]),
        json!([{"name":"init","description":"ok","source":"unknown"}]),
        json!([{"name":"","description":"ok"}]),
        json!([{"name":"init"}]),
        json!([{"name":"init","description":false}]),
        json!([{"name":"init","description":"ok","inputHint":null}]),
        json!(null),
    ] {
        assert!(parse_commands(&invalid).is_err(), "{invalid}");
    }
}

#[gpui::test]
fn slash_selection_revalidates_owner_query_composer_and_attachment_ownership(
    cx: &mut TestAppContext,
) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].started = true;
        s.workspaces[0].inbound = Some(tx);
        s.active_workspace = Some(key.clone());
        let query = s.workspaces[0].slash_catalogs.entry(None).or_default();
        query.start();
        query.finish(parse_commands(&commands()).unwrap());
        s.composer.update(cx, |c, _| {
            c.set_text("/in");
            c.attachments = vec![crate::composer::attachment::AttachmentRef {
                reference: "retained".into(),
                file_name: "notes.txt".into(),
                mime: "text/plain".into(),
                bytes: 1,
                preview_ref: None,
            }];
            c.temp_owned = vec![std::path::PathBuf::from("owned-paste")];
        });
        let receipt = s.slash_catalog_receipt(cx).unwrap();
        let command = parse_commands(&commands()).unwrap().remove(0);
        assert!(s.apply_slash_suggestion(&receipt, &command, cx));
        assert_eq!(s.composer.read(cx).text(), "/init ");
        assert_eq!(s.composer.read(cx).attachments()[0].reference, "retained");
        assert_eq!(
            s.composer.read(cx).temp_owned,
            vec![std::path::PathBuf::from("owned-paste")]
        );
        assert!(rx.try_recv().is_err());
        assert!(!s.apply_slash_suggestion(&receipt, &command, cx));
        for mutation in 0..7 {
            s.active = None;
            s.viewing_child = None;
            s.workspaces[0]
                .slash_catalogs
                .get_mut(&None)
                .unwrap()
                .finish(parse_commands(&commands()).unwrap());
            s.composer.update(cx, |c, _| c.set_text("/in"));
            let receipt = s.slash_catalog_receipt(cx).unwrap();
            match mutation {
                0 => s.active = Some("another".into()),
                1 => s.navigation_generation += 1,
                2 => s.workspaces[0].generation += 1,
                3 => s.workspaces[0]
                    .slash_catalogs
                    .get_mut(&None)
                    .unwrap()
                    .start(),
                4 => s.composer.update(cx, |c, _| c.set_text("newer draft")),
                5 => s.viewing_child = Some("child".into()),
                _ => s.composer.update(cx, |c, _| c.marked_utf16 = Some(0..3)),
            }
            let before = s.composer.read(cx).text().to_owned();
            assert!(!s.apply_slash_suggestion(&receipt, &command, cx));
            s.refresh_slash_catalog(&receipt, cx);
            assert!(rx.try_recv().is_err());
            assert_eq!(s.composer.read(cx).text(), before);
        }
    });
}
