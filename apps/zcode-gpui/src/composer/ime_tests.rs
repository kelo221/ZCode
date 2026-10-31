use crate::composer::input::Composer;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
fn text_for_range_clamps_past_the_end(cx: &mut TestAppContext) {
    let composer = cx.new(Composer::new);
    composer.update(cx, |c, _| {
        // Android probes an empty field with 0..1; it must read as an editor.
        assert_eq!(c.clamped_text_utf16(0..1), (String::new(), 0..0));

        c.set_text("hi é");
        // Mirror window: caret ± 256 must still return the text.
        assert_eq!(c.clamped_text_utf16(0..260), ("hi é".to_string(), 0..4));
        assert_eq!(c.clamped_text_utf16(3..4), ("é".to_string(), 3..4));
        assert_eq!(c.clamped_text_utf16(9..12), (String::new(), 4..4));
    });
}
