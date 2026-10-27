//! Legacy color token IDs, theme mode and the typography ladder. The constants below
//! are only identities resolved by `theme_colors::color` to Desktop's Zai palette
//! (`theme_palette.rs`); their numeric values are not the colors drawn.

#![allow(dead_code)]

use gpui::{AnyElement, Div, IntoElement, div, prelude::*, px};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// Families registered by `ely_gpui_component::init` — use these, not system
/// fonts, so every surface matches the ely components.
pub const UI_FONT: &str = "Inter";
pub const MONO_FONT: &str = "JetBrains Mono";

// Legacy token identities (resolved through theme_colors::color).
pub const BG: u32 = 0x131110;
pub const PANEL: u32 = 0x1b1917;
pub const CARD: u32 = 0x201f1d;
pub const CARD_HOVER: u32 = 0x2a2927;
pub const BORDER: u32 = 0x2c2b29;
pub const HOVER: u32 = 0x1f1e1c;
pub const SELECTED: u32 = 0x262524;
pub const TEXT: u32 = 0xf3f1f0;
pub const MUTED: u32 = 0x94928f;
pub const ACCENT: u32 = 0x2dd4bf;
pub const USER_BLUE: u32 = 0x77abec;
pub const REASONING: u32 = 0xa78bfa;
pub const TOOL: u32 = 0xe8b45e;
pub const AMBER: u32 = 0xe8b45e;
pub const WARNING: u32 = 0xe8b45e;
pub const DANGER: u32 = 0xe5756e;
pub const SUCCESS: u32 = 0x6bbc89;
pub const PRIMARY: u32 = 0xf3f1f0;
pub const CODE_BG: u32 = 0x1a1816;
pub const CODE_BORDER: u32 = 0x262423;
pub const CODE_HEADER: u32 = 0x232120;
pub const LINK: u32 = 0x77abec;
pub const DIFF_ADD_BG: u32 = 0x182c1f;
pub const DIFF_DEL_BG: u32 = 0x3a1d1b;
pub const DIFF_HUNK_BG: u32 = 0x19273a;
pub const SIDEBAR: u32 = 0x161616;
pub const SURFACE: u32 = 0x252525;
pub const INPUT: u32 = 0x2b2b2b;
pub const TAG: u32 = 0x363636;
pub const SUBTLEST: u32 = 0x5a5a5a;
pub const BORDER_HOVER: u32 = 0x3a3a3a;
pub const FRAME: u32 = 0x2b2b2c;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    ZaiDark,
    ZaiLight,
    System,
}

impl ThemeMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "light" | "zai-light" => Self::ZaiLight,
            "dark" | "zai-dark" => Self::ZaiDark,
            _ => Self::System,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ZaiDark => "zai-dark",
            Self::ZaiLight => "zai-light",
            Self::System => "system",
        }
    }
}

pub use crate::shared::theme_palette::ThemePalette;

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(2); // preference: dark, light, system
static EFFECTIVE_LIGHT: AtomicU8 = AtomicU8::new(0);

pub fn set_theme_mode(mode: ThemeMode) {
    let val = match mode {
        ThemeMode::ZaiLight => 1,
        ThemeMode::ZaiDark => 0,
        ThemeMode::System => 2,
    };
    ACTIVE_THEME.store(val, Ordering::Relaxed);
}

pub fn theme_mode() -> ThemeMode {
    match ACTIVE_THEME.load(Ordering::Relaxed) {
        1 => ThemeMode::ZaiLight,
        0 => ThemeMode::ZaiDark,
        _ => ThemeMode::System,
    }
}

pub fn effective_light() -> bool {
    EFFECTIVE_LIGHT.load(Ordering::Relaxed) == 1
}

pub fn active_theme() -> ThemePalette {
    if EFFECTIVE_LIGHT.load(Ordering::Relaxed) == 1 {
        ThemePalette::zai_light()
    } else {
        ThemePalette::zai_dark()
    }
}

pub fn apply_appearance(appearance: gpui::WindowAppearance, cx: &mut gpui::App) {
    let system_light = matches!(
        appearance,
        gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight
    );
    let light = match theme_mode() {
        ThemeMode::ZaiLight => true,
        ThemeMode::ZaiDark => false,
        ThemeMode::System => system_light,
    };
    EFFECTIVE_LIGHT.store(u8::from(light), Ordering::Relaxed);
    use ely_gpui_component::theme::{Mode, Theme};
    Theme::set_palette(
        Mode::Light,
        Some(ThemePalette::zai_light().ely_palette(true)),
        cx,
    );
    Theme::set_palette(
        Mode::Dark,
        Some(ThemePalette::zai_dark().ely_palette(false)),
        cx,
    );
    ely_gpui_component::theme::Theme::set_mode_now(
        if light {
            ely_gpui_component::theme::Mode::Light
        } else {
            ely_gpui_component::theme::Mode::Dark
        },
        cx,
    );
}

pub fn ui_size(original: f32) -> f32 {
    original + font_size_base() - 14.0
}

// ============================================================================
// UI Font Scaling Ladder (DESIGN.md)
// Base font size: 14px default, clamped between 12px and 20px.
// ============================================================================

pub(crate) const DEFAULT_UI_FONT_SIZE: f32 = 14.0;
pub(crate) const MIN_UI_FONT_SIZE: f32 = 12.0;
pub(crate) const MAX_UI_FONT_SIZE: f32 = 20.0;

// Stored as u32 bits of f32 for atomic updates
static UI_FONT_SIZE_BITS: AtomicU32 = AtomicU32::new(DEFAULT_UI_FONT_SIZE.to_bits());

pub fn set_ui_font_size(size: f32) {
    let clamped = size.clamp(MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE);
    UI_FONT_SIZE_BITS.store(clamped.to_bits(), Ordering::Relaxed);
}

pub fn font_size_base() -> f32 {
    f32::from_bits(UI_FONT_SIZE_BITS.load(Ordering::Relaxed))
}

pub fn font_size_xl() -> f32 {
    font_size_base() + 4.0
}

pub fn font_size_lg() -> f32 {
    font_size_base() + 2.0
}

pub fn font_size_caption() -> f32 {
    font_size_base() - 1.0
}

pub fn font_size_sm() -> f32 {
    font_size_base() - 2.0
}

pub fn font_size_xs() -> f32 {
    font_size_base() - 4.0
}

pub fn font_size_2xs() -> f32 {
    font_size_base() - 5.0
}

// Lucide icons bundled with Ely, the set Desktop uses (lucide-react).
pub use ely_gpui_component::primitives::IconName;
pub const I_ADD: IconName = IconName::Plus;
pub const I_SEARCH: IconName = IconName::Search;
pub const I_FOLDER: IconName = IconName::Folder;
pub const I_CHEVRON_DOWN: IconName = IconName::ChevronDown;
pub const I_CHEVRON_RIGHT: IconName = IconName::ChevronRight;
pub const I_ARROW_UP: IconName = IconName::ArrowUp;
pub const I_ARROW_LEFT: IconName = IconName::ArrowLeft;
pub const I_STOP: IconName = IconName::Square;
pub const I_EDIT: IconName = IconName::Pencil;
pub const I_CLOSE: IconName = IconName::X;
pub const I_SHIELD: IconName = IconName::Shield;
pub const I_BULB: IconName = IconName::Lightbulb;

pub fn icon(name: IconName, size: f32, color: u32) -> Div {
    div().flex_none().child(
        gpui::svg()
            .path(name.path())
            .size(px(size))
            .text_color(crate::shared::theme_colors::color(color)),
    )
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
            .text_size(px(ui_size(11.)))
            .px_1p5()
            .rounded_sm()
            .bg(crate::shared::theme_colors::color(CARD))
            .text_color(crate::shared::theme_colors::color(color))
            .child(label)
            .into_any_element(),
    )
}
