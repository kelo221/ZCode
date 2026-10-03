//! Tests for `transcript/list.rs`: list reconciliation planning.

use super::*;

#[test]
fn identical_ids_need_no_splice() {
    assert_eq!(
        plan_list_edit(&[1, 2, 3], &[1, 2, 3], false),
        ListEdit::Unchanged
    );
}

#[test]
fn streaming_appends_splice_at_tail() {
    assert_eq!(
        plan_list_edit(&[1, 2], &[1, 2, 3, 4], false),
        ListEdit::Append(2)
    );
    assert_eq!(plan_list_edit(&[], &[1], false), ListEdit::Append(1));
}

#[test]
fn history_pages_splice_at_head() {
    assert_eq!(
        plan_list_edit(&[40, 41], &[38, 39, 40, 41], false),
        ListEdit::Prepend(2)
    );
}

#[test]
fn session_switch_and_replacement_reset() {
    assert_eq!(plan_list_edit(&[1, 2], &[1, 2, 3], true), ListEdit::Reset);
    // Truncation (row.removed) and in-place replacement cannot splice.
    assert_eq!(plan_list_edit(&[1, 2, 3], &[1, 2], false), ListEdit::Reset);
    assert_eq!(
        plan_list_edit(&[1, 2, 3], &[1, 5, 6], false),
        ListEdit::Reset
    );
    // An empty previous index is an append, never a prepend.
    assert_eq!(plan_list_edit(&[], &[], false), ListEdit::Unchanged);
}

#[test]
fn tail_pin_targets_last_row_and_never_underflows() {
    assert!(tail_pin(0).is_none());
    let pin = tail_pin(5).unwrap();
    assert_eq!(pin.item_ix, 4);
    assert!(pin.offset_in_item > px(1.0e6));
}

/// Prepend splice keeps the anchored row: gpui shifts `logical_scroll_top`
/// by the spliced count, so the row the reader was on stays visible.
#[test]
fn prepend_splice_shifts_scroll_anchor() {
    let state = ListState::new(3, ListAlignment::Top, px(OVERDRAW));
    state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(7.),
    });
    state.splice(0..0, 2);
    let top = state.logical_scroll_top();
    assert_eq!(top.item_ix, 3);
    assert_eq!(top.offset_in_item, px(7.));
    assert_eq!(state.item_count(), 5);
}
