//! History paging: `rows.firstRowId` vs the loaded window ("Load earlier messages").

use super::*;
use serde_json::json;

#[test]
fn merges_older_history_pages() {
    // Wire shape (CLI getWireSnapshotForProfile): `firstRowId` is the oldest
    // row of the whole conversation, the window is only the tail.
    let mut c = ConversationState::default();
    c.apply_snapshot(&json!({ "rows": { "firstRowId": 1, "totalCount": 50,
        "window": [ { "rowId": 40, "kind": "userInput", "text": "newest" } ] } }));
    assert!(c.has_more_history());
    assert_eq!(c.loaded_first_row_id(), Some(40));
    c.apply_older_rows(
        &[
            json!({ "rowId": 38, "kind": "userInput", "text": "older" }),
            json!({ "rowId": 39, "kind": "userInput", "text": "between" }),
        ],
        Some(true),
    );
    assert_eq!(
        c.first_row_id,
        Some(1),
        "the conversation start is unchanged"
    );
    assert_eq!(c.loaded_first_row_id(), Some(38));
    assert!(c.has_more_history());
    // rowId ordering is guaranteed by the BTreeMap.
    let ids: Vec<u64> = c.rows.keys().copied().collect();
    assert_eq!(ids, vec![38, 39, 40]);
    c.apply_older_rows(
        &[json!({ "rowId": 1, "kind": "userInput", "text": "first" })],
        Some(false),
    );
    assert!(!c.has_more_history(), "reached the start");
}

#[test]
fn server_end_of_history_hides_load_earlier_even_with_gaps() {
    // Hidden rows (filtered per delivery profile) can leave the oldest
    // visible row newer than `firstRowId`; `hasMore: false` settles it.
    let mut c = ConversationState::default();
    c.apply_snapshot(&json!({ "rows": { "firstRowId": 1, "totalCount": 3,
        "window": [ { "rowId": 5, "kind": "userInput", "text": "tail" } ] } }));
    c.apply_older_rows(
        &[json!({ "rowId": 3, "kind": "userInput", "text": "x" })],
        Some(false),
    );
    assert!(!c.has_more_history());
}

#[test]
fn short_or_empty_conversations_offer_no_earlier_history() {
    let mut c = ConversationState::default();
    assert!(!c.has_more_history(), "no snapshot yet");
    c.apply_snapshot(&json!({ "rows": { "firstRowId": null, "totalCount": 0, "window": [] } }));
    assert!(!c.has_more_history());
    c.apply_deltas(&[json!({ "op": "row.appended",
        "row": { "rowId": 7, "kind": "userInput", "text": "hi" } })]);
    assert_eq!(c.first_row_id, Some(7));
    assert!(!c.has_more_history());
    // Rewinding from the oldest row empties the branch (shared reducer parity).
    c.apply_deltas(&[json!({ "op": "row.removed", "fromRowId": 7 })]);
    assert_eq!(c.first_row_id, None);
}
