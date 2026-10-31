//! GCode color tokens. Field names follow Desktop's CSS variables (`--color-<name>`,
//! `packages/ui/src/styles.css`); translucent values keep their alpha so they layer
//! over whatever surface they sit on. Values are the Dracula palette (BRANDING.md).

use gpui::{Hsla, Rgba, rgb, rgba};

/// Dracula's 16-colour terminal palette (black … white, then the bright row).
pub const DRACULA_ANSI: [u32; 16] = [
    0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2, 0x6272a4,
    0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePalette {
    pub background: Rgba,
    pub sidebar: Rgba,
    /// Windows window frame (`--color-background-win-alt`): Desktop paints the
    /// opaque root surface, sidebar included, with it.
    pub window_frame: Rgba,
    pub panel: Rgba,
    pub card: Rgba,
    pub surface: Rgba,
    pub surface_hover: Rgba,
    pub hover: Rgba,
    pub selected: Rgba,
    pub border: Rgba,
    pub border_hover: Rgba,
    pub input: Rgba,
    pub menu: Rgba,
    pub menu_hover: Rgba,
    /// Also `--color-markdown-inline-code`.
    pub tag: Rgba,
    pub foreground: Rgba,
    pub subtle: Rgba,
    pub subtlest: Rgba,
    pub inverse: Rgba,
    /// Submit button and other brand fills, drawn with `inverse` text.
    pub brand: Rgba,
    /// Selection-style tint (`--color-accent`), not a highlight color.
    pub accent: Rgba,
    pub info: Rgba,
    pub destructive: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub link: Rgba,
    pub reasoning: Rgba,
    pub tool: Rgba,
    pub user: Rgba,
    pub diff_add_bg: Rgba,
    pub diff_del_bg: Rgba,
    pub diff_hunk_bg: Rgba,
    pub selection: Rgba,
}

impl ThemePalette {
    /// GCode's only palette: Dracula (https://draculatheme.com/contribute).
    /// Background, Current Line, Foreground, Comment and the seven accents are the
    /// spec values; the darker sidebar/frame shades are Dracula's own UI variants.
    pub fn dracula() -> Self {
        Self {
            background: rgba(0x282a36ff),
            sidebar: rgba(0x21222cff),
            window_frame: rgba(0x191a21ff),
            panel: rgba(0x21222cff),
            card: rgba(0x343746ff),
            surface: rgba(0xf8f8f20d),
            surface_hover: rgba(0xf8f8f21a),
            hover: rgba(0xf8f8f20d),
            selected: rgba(0x44475aff),
            border: rgba(0x44475aff),
            border_hover: rgba(0x6272a4ff),
            input: rgba(0x21222cff),
            menu: rgba(0x343746ff),
            menu_hover: rgba(0x44475aff),
            tag: rgba(0x44475aff),
            foreground: rgba(0xf8f8f2ff),
            subtle: rgba(0xf8f8f299),
            subtlest: rgba(0x6272a4ff),
            inverse: rgba(0x282a36ff),
            brand: rgba(0xbd93f9ff),
            accent: rgba(0x44475aff),
            info: rgba(0x8be9fdff),
            destructive: rgba(0xff5555ff),
            success: rgba(0x50fa7bff),
            warning: rgba(0xffb86cff),
            link: rgba(0x8be9fdff),
            reasoning: rgba(0xbd93f9ff),
            tool: rgba(0xf1fa8cff),
            user: rgba(0xff79c6ff),
            diff_add_bg: rgba(0x50fa7b26),
            diff_del_bg: rgba(0xff555526),
            diff_hunk_bg: rgba(0xbd93f91f),
            selection: rgba(0x44475aff),
        }
    }

    /// Ely's semantic palette built from the same tokens, so Ely controls match the
    /// hand-drawn surfaces. Fields without a Desktop counterpart keep Ely's values.
    pub fn ely_palette(&self) -> ely_gpui_component::theme::Palette {
        let mut out = ely_gpui_component::theme::Palette::dark(false);
        let h = |c: Rgba| -> Hsla { c.into() };
        out.bg = h(self.background);
        out.surface = h(self.card);
        out.sunken = h(self.input);
        out.overlay = h(self.menu);
        out.hover = h(self.hover);
        out.active = h(self.selected);
        out.border = h(self.border);
        out.border_strong = h(self.border_hover);
        out.fg = h(self.foreground);
        out.fg_muted = h(self.subtle);
        out.fg_subtle = h(self.subtlest);
        out.fg_disabled = h(self.subtlest);
        // Desktop's primary button is `bg-primary`: the brand fill with inverse text.
        out.accent = h(self.brand);
        out.accent_hover = h(self.brand).opacity(0.8);
        out.on_accent = h(self.inverse);
        out.focus = h(self.border_hover);
        out.link = h(self.link);
        out.selection = h(self.selection);
        out.success = h(self.success);
        out.warning = h(self.warning);
        out.danger = h(self.destructive);
        out.info = h(self.info);
        out.success_subtle = h(self.diff_add_bg);
        out.danger_subtle = h(self.diff_del_bg);
        out.info_subtle = h(self.diff_hunk_bg);
        out.tooltip_bg = h(self.menu);
        out.tooltip_fg = h(self.foreground);
        let hex = |c: u32| -> Hsla { rgb(c).into() };
        out.ansi = DRACULA_ANSI.map(hex);
        out.chart = [
            0xbd93f9, 0x8be9fd, 0x50fa7b, 0xffb86c, 0xff79c6, 0xf1fa8c, 0xff5555, 0x6272a4,
        ]
        .map(hex);
        out.syntax = ely_gpui_component::theme::Syntax {
            keyword: hex(0xff79c6),
            string: hex(0xf1fa8c),
            number: hex(0xbd93f9),
            comment: hex(0x6272a4),
            function: hex(0x50fa7b),
            type_name: hex(0x8be9fd),
            constant: hex(0xbd93f9),
            property: hex(0xffb86c),
            tag: hex(0xff79c6),
            attribute: hex(0x50fa7b),
            operator: hex(0xff79c6),
            punctuation: hex(0xf8f8f2),
            variable: hex(0xf8f8f2),
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dracula_tokens_match_the_spec() {
        let p = ThemePalette::dracula();
        assert_eq!(p.background, rgba(0x282a36ff));
        assert_eq!(p.selection, rgba(0x44475aff));
        assert_eq!(p.foreground, rgba(0xf8f8f2ff));
        assert_eq!(p.subtlest, rgba(0x6272a4ff));
        assert_eq!(p.brand, rgba(0xbd93f9ff));
        assert_eq!(p.destructive, rgba(0xff5555ff));
    }

    #[test]
    fn ely_primary_uses_brand_fill_with_inverse_text() {
        let p = ThemePalette::dracula();
        let ely = p.ely_palette();
        assert_eq!(ely.accent, Hsla::from(p.brand));
        assert_eq!(ely.on_accent, Hsla::from(p.inverse));
        assert_eq!(ely.bg, Hsla::from(p.background));
    }
}
