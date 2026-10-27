use super::*;

#[test]
fn end_stops_before_the_space_a_soft_wrap_broke_at() {
    let text = "hello world again";
    // A wrap after "hello " puts the boundary at 6, the start of "world".
    assert_eq!(end_before_wrap(text, 6), 5);
    assert_eq!(end_before_wrap(text, text.len()), text.len(), "end of text");
    assert_eq!(end_before_wrap("ab\ncd", 2), 2, "a hard line end stays put");
    assert_eq!(
        end_before_wrap("abcdef", 3),
        3,
        "a mid-word break stays put"
    );
}
