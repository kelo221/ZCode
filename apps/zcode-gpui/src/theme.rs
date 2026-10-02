//! Color tokens mirroring the Zai dark theme (packages/ui/src/styles.css).

use gpui::{AnyElement, IntoElement, div, prelude::*, px, rgb};

pub const BG: u32 = 0x161616;
pub const PANEL: u32 = 0x202020;
pub const CARD: u32 = 0x2b2b2b;
pub const BORDER: u32 = 0x2a2a2a;
pub const TEXT: u32 = 0xececec;
pub const MUTED: u32 = 0x8f8f8f;
pub const ACCENT: u32 = 0x2dd4bf;
pub const USER_BLUE: u32 = 0x60a5fa;
pub const REASONING: u32 = 0xa78bfa;
pub const TOOL: u32 = 0xf59e0b;
pub const AMBER: u32 = 0xf59e0b;
pub const DANGER: u32 = 0xff5c5c;
pub const SUCCESS: u32 = 0x22c55e;

pub fn phase_badge(phase: &str) -> AnyElement {
    let color = match phase {
        "running" => ACCENT,
        "completedSuccess" => MUTED,
        "failed" | "error" => DANGER,
        _ => MUTED,
    };
    div()
        .text_size(px(10.))
        .px_1p5()
        .rounded_sm()
        .bg(rgb(BORDER))
        .text_color(rgb(color))
        .child(phase.to_string())
        .into_any_element()
}
