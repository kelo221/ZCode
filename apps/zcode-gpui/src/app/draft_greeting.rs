//! Draft (empty conversation) greeting from Desktop `ConversationDraftEmptyState`:
//! a time-of-day line over the outlined ZCode logo, centred above the composer.

use crate::shared::app_assets::EMPTY_LOGO;
use crate::shared::theme::{SUBTLEST, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, div, prelude::*, px};

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
pub(crate) fn draft_greeting(column: f32) -> AnyElement {
    let logo_width = column.min(400.);
    div()
        .relative()
        .w_full()
        .mb_10()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .top(px(-logo_width * 0.4 - 20.))
                .left(px((column - logo_width) / 2.))
                .opacity(0.5)
                // Desktop masks the outline to transparent by 70% of its height.
                .h(px(logo_width * 0.8 * 0.6))
                .overflow_hidden()
                .child(
                    gpui::svg()
                        .path(EMPTY_LOGO)
                        .w(px(logo_width))
                        .h(px(logo_width * 0.8))
                        .text_color(rgb(SUBTLEST)),
                ),
        )
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
