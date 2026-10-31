//! Draft (empty conversation) greeting from Desktop `ConversationDraftEmptyState`:
//! the GCode mark over a time-of-day line, centred above the composer.

use crate::shared::app_assets::GCODE_MARK_SVG;
use crate::shared::theme::TEXT;
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, App, RenderImage, div, prelude::*, px};
use std::cell::RefCell;
use std::sync::Arc;

/// Last rasterised mark, keyed by (size, scale factor) bits.
type MarkCache = Option<((u32, u32), Arc<RenderImage>)>;

/// The mark rasterised at exactly `size` logical px × `scale_factor`. `img()` with
/// an SVG path rasterises at the intrinsic size and stretches it, which shows
/// pixels on scaled displays.
fn mark_image(size: f32, scale_factor: f32, cx: &App) -> Option<Arc<RenderImage>> {
    thread_local! {
        static CACHE: RefCell<MarkCache> = const { RefCell::new(None) };
    }
    let key = (size.to_bits(), scale_factor.to_bits());
    CACHE.with(|cache| {
        if let Some((cached, image)) = cache.borrow().as_ref()
            && *cached == key
        {
            return Some(image.clone());
        }
        let image = cx
            .svg_renderer()
            .render_single_frame(GCODE_MARK_SVG, scale_factor * size / 64.)
            .ok()?;
        *cache.borrow_mut() = Some((key, image.clone()));
        Some(image)
    })
}

/// Desktop's hour buckets (`getChatEmptyGreetingMessageId`).
pub(crate) fn greeting_key(hour: u32) -> &'static str {
    match hour {
        5..=8 => "chat.empty.greeting.morningEarly",
        9..=11 => "chat.empty.greeting.morning",
        12..=13 => "chat.empty.greeting.noon",
        14..=17 => "chat.empty.greeting.afternoon",
        18..=22 => "chat.empty.greeting.evening",
        _ => "chat.empty.greeting.lateNight",
    }
}

#[cfg(windows)]
fn local_hour() -> u32 {
    // SAFETY: GetLocalTime only writes the returned SYSTEMTIME.
    u32::from(unsafe { windows::Win32::System::SystemInformation::GetLocalTime() }.wHour)
}

#[cfg(not(windows))]
fn local_hour() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    ((secs / 3600) % 24) as u32
}

/// Greeting block for a conversation column `column` px wide. Desktop shrinks
/// the 30px line to fit; narrow columns step down to 24px.
pub(crate) fn draft_greeting(column: f32, scale_factor: f32, cx: &App) -> AnyElement {
    let mark = if column >= 520. { 64. } else { 48. };
    div()
        .w_full()
        .mb_10()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .when_some(mark_image(mark, scale_factor, cx), |el, image| {
            el.child(gpui::img(image).size(px(mark)).mb_5())
        })
        .child(
            div()
                .w_full()
                .px_4()
                .text_center()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_size(px(if column >= 520. { 30. } else { 24. }))
                .line_height(gpui::relative(1.2))
                .text_color(rgb(TEXT))
                .child(crate::shared::i18n::t(greeting_key(local_hour())).to_string()),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::greeting_key;

    #[test]
    fn greeting_buckets_match_desktop_boundaries() {
        assert_eq!(greeting_key(4), "chat.empty.greeting.lateNight");
        assert_eq!(greeting_key(5), "chat.empty.greeting.morningEarly");
        assert_eq!(greeting_key(9), "chat.empty.greeting.morning");
        assert_eq!(greeting_key(12), "chat.empty.greeting.noon");
        assert_eq!(greeting_key(14), "chat.empty.greeting.afternoon");
        assert_eq!(greeting_key(18), "chat.empty.greeting.evening");
        assert_eq!(greeting_key(23), "chat.empty.greeting.lateNight");
    }
}
