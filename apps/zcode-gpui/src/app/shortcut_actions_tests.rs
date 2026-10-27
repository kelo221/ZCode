use super::*;

#[test]
fn zoom_steps_by_a_pixel_within_the_slider_range() {
    assert_eq!(zoom_target(14., Some(1.)), 15.);
    assert_eq!(zoom_target(14., Some(-1.)), 13.);
    assert_eq!(zoom_target(20., Some(1.)), 20., "clamped at the top");
    assert_eq!(zoom_target(12., Some(-1.)), 12., "clamped at the bottom");
    assert_eq!(zoom_target(18., None), 14., "reset");
}

#[test]
fn adjacent_conversation_wraps_and_starts_at_an_end() {
    let order: Vec<(String, String)> = ["a", "b", "c"]
        .iter()
        .map(|s| ("ws".to_string(), s.to_string()))
        .collect();
    assert_eq!(adjacent(&order, Some("b"), true), Some(2));
    assert_eq!(adjacent(&order, Some("c"), true), Some(0), "wraps forward");
    assert_eq!(adjacent(&order, Some("a"), false), Some(2), "wraps back");
    assert_eq!(adjacent(&order, None, true), Some(0));
    assert_eq!(adjacent(&order, Some("gone"), false), Some(2));
    assert_eq!(adjacent(&[], Some("a"), true), None);
}
