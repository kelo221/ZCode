//! Color tokens mirroring the Zai dark theme (packages/ui/src/styles.css,
//! `[data-theme="zai-dark"]`): sidebar/background #161616, panel #202020,
//! card/input #2b2b2b, borders at 10% white.

use gpui::{AnyElement, Div, IntoElement, div, prelude::*, px, rgb};

/// App background and sidebar (`--color-background`, `--color-sidebar`).
pub const BG: u32 = 0x161616;
/// Header / main panel (`--color-panel`).
pub const PANEL: u32 = 0x202020;
/// Card, input and popover surface (`--color-card`, `--color-input`).
pub const CARD: u32 = 0x2b2b2b;
/// Menu hover (`--color-menu-hover`).
pub const CARD_HOVER: u32 = 0x363636;
/// 10% white over the dark surfaces (`--color-border`).
pub const BORDER: u32 = 0x343434;
/// Sidebar row hover / selection (5% / 10% white over #161616).
pub const HOVER: u32 = 0x232323;
pub const SELECTED: u32 = 0x2d2d2d;
/// `--color-foreground` (neutral-200) and muted text.
pub const TEXT: u32 = 0xe5e5e5;
pub const MUTED: u32 = 0x8f8f8f;
pub const ACCENT: u32 = 0x2dd4bf;
pub const USER_BLUE: u32 = 0x60a5fa;
pub const REASONING: u32 = 0xa78bfa;
pub const TOOL: u32 = 0xf59e0b;
pub const AMBER: u32 = 0xf59e0b;
/// `--color-warning` (yellow-500), used for the Full access mode.
pub const WARNING: u32 = 0xeab308;
pub const DANGER: u32 = 0xff5c5c;
pub const SUCCESS: u32 = 0x22c55e;
/// `--color-primary`: the round send button.
pub const PRIMARY: u32 = 0xffffff;

/// Windows icon font (ships with Windows 10+); glyph code points below.
const ICON_FONT: &str = "Segoe MDL2 Assets";
pub const I_ADD: char = '\u{E710}';
pub const I_FOLDER: char = '\u{F12B}';
pub const I_CHEVRON_DOWN: char = '\u{E70D}';
pub const I_ARROW_UP: char = '\u{E74A}';
pub const I_STOP: char = '\u{E71A}';
pub const I_EDIT: char = '\u{E70F}';
pub const I_CLOSE: char = '\u{E711}';
pub const I_SHIELD: char = '\u{EA18}';
pub const I_BULB: char = '\u{EA80}';

pub fn icon(glyph: char, size: f32, color: u32) -> Div {
    div()
        .font_family(ICON_FONT)
        .text_size(px(size))
        .text_color(rgb(color))
        .child(glyph.to_string())
}

/// Phase chip, shown only while a session is live or failed.
pub fn phase_badge(phase: &str) -> Option<AnyElement> {
    let (label, color) = match phase {
        "running" | "prewarming" => ("Running", ACCENT),
        "error" => ("Error", DANGER),
        _ => return None,
    };
    Some(
        div()
            .text_size(px(11.))
            .px_1p5()
            .rounded_sm()
            .bg(rgb(CARD))
            .text_color(rgb(color))
            .child(label)
            .into_any_element(),
    )
}
