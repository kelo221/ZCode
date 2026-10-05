//! Color tokens and typography ladder. Neutrals + semantic colors follow the
//! ely-gpui-component palette (the design system this app now renders with),
//! so hand-rolled surfaces and ely components read as one; brand accents
//! (teal accent, violet reasoning) keep the Zai identity (DESIGN.md).

#![allow(dead_code)]

use gpui::{AnyElement, Div, IntoElement, div, prelude::*, px, rgb};
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

/// Families registered by `ely_gpui_component::init` — use these, not system
/// fonts, so every surface matches the ely components.
pub const UI_FONT: &str = "Inter";
pub const MONO_FONT: &str = "JetBrains Mono";

// Default static tokens (ely warm dark fallback)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub bg: u32,
    pub panel: u32,
    pub card: u32,
    pub card_hover: u32,
    pub border: u32,
    pub hover: u32,
    pub selected: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub user_blue: u32,
    pub reasoning: u32,
    pub tool: u32,
    pub warning: u32,
    pub danger: u32,
    pub success: u32,
    pub primary: u32,
    pub code_bg: u32,
    pub code_border: u32,
    pub code_header: u32,
    pub link: u32,
    pub diff_add_bg: u32,
    pub diff_del_bg: u32,
    pub diff_hunk_bg: u32,
}

impl ThemePalette {
    pub const fn zai_dark() -> Self {
        Self {
            bg: BG,
            panel: PANEL,
            card: CARD,
            card_hover: CARD_HOVER,
            border: BORDER,
            hover: HOVER,
            selected: SELECTED,
            text: TEXT,
            muted: MUTED,
            accent: ACCENT,
            user_blue: USER_BLUE,
            reasoning: REASONING,
            tool: TOOL,
            warning: WARNING,
            danger: DANGER,
            success: SUCCESS,
            primary: PRIMARY,
            code_bg: CODE_BG,
            code_border: CODE_BORDER,
            code_header: CODE_HEADER,
            link: LINK,
            diff_add_bg: DIFF_ADD_BG,
            diff_del_bg: DIFF_DEL_BG,
            diff_hunk_bg: DIFF_HUNK_BG,
        }
    }

    pub const fn zai_light() -> Self {
        Self {
            bg: 0xfcfaf7,
            panel: 0xffffff,
            card: 0xffffff,
            card_hover: 0xf3f0ec,
            border: 0xe1dfdd,
            hover: 0xf5f2ee,
            selected: 0xece9e4,
            text: 0x181613,
            muted: 0x605d5a,
            accent: 0x0f766e,
            user_blue: 0x2e69b2,
            reasoning: 0x7c3aed,
            tool: 0xa25f12,
            warning: 0xa25f12,
            danger: 0xba3e38,
            success: 0x267b4c,
            primary: 0x181613,
            code_bg: 0xf6f4f0,
            code_border: 0xe4e1dd,
            code_header: 0xeeeae5,
            link: 0x2e69b2,
            diff_add_bg: 0xe7f9ec,
            diff_del_bg: 0xffefec,
            diff_hunk_bg: 0xebf4ff,
        }
    }
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(0); // 0 = ZaiDark, 1 = ZaiLight

pub fn set_theme_mode(mode: ThemeMode) {
    let val = match mode {
        ThemeMode::ZaiLight => 1,
        _ => 0,
    };
    ACTIVE_THEME.store(val, Ordering::Relaxed);
}

pub fn theme_mode() -> ThemeMode {
    match ACTIVE_THEME.load(Ordering::Relaxed) {
        1 => ThemeMode::ZaiLight,
        _ => ThemeMode::ZaiDark,
    }
}

pub fn active_theme() -> ThemePalette {
    match theme_mode() {
        ThemeMode::ZaiLight => ThemePalette::zai_light(),
        _ => ThemePalette::zai_dark(),
    }
}

// ============================================================================
// UI Font Scaling Ladder (DESIGN.md)
// Base font size: 14px default, clamped between 12px and 20px.
// ============================================================================

const DEFAULT_UI_FONT_SIZE: f32 = 14.0;
const MIN_UI_FONT_SIZE: f32 = 12.0;
const MAX_UI_FONT_SIZE: f32 = 20.0;

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

/// Windows icon font (ships with Windows 10+); glyph code points below.
const ICON_FONT: &str = "Segoe MDL2 Assets";
pub const I_ADD: char = '\u{E710}';
pub const I_SEARCH: char = '\u{E721}';
pub const I_FOLDER: char = '\u{F12B}';
pub const I_CHEVRON_DOWN: char = '\u{E70D}';
pub const I_CHEVRON_RIGHT: char = '\u{E70E}';
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
