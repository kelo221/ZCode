//! Desktop's Zai color tokens (`packages/ui/src/styles.css`, `.theme-zai-dark` and
//! `.theme-zai-light`, plus the `.dark`/`:root` values they inherit). Field names follow
//! the CSS variables (`--color-<name>`); translucent values keep their alpha so they
//! layer over whatever surface they sit on, as in the Desktop app.

use gpui::{Hsla, Rgba, rgba};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePalette {
    pub background: Rgba,
    pub sidebar: Rgba,
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
    /// Submit button and other brand fills: white in dark, black in light.
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
    pub fn zai_dark() -> Self {
        Self {
            background: rgba(0x161616ff),
            sidebar: rgba(0x161616ff),
            panel: rgba(0x202020ff),
            card: rgba(0x2b2b2bff),
            surface: rgba(0xffffff0d),
            surface_hover: rgba(0xffffff1a),
            hover: rgba(0xffffff0d),
            selected: rgba(0xffffff1a),
            border: rgba(0xffffff1a),
            border_hover: rgba(0xffffff26),
            input: rgba(0x2b2b2bff),
            menu: rgba(0x2b2b2bff),
            menu_hover: rgba(0x363636ff),
            tag: rgba(0x363636ff),
            // neutral-300 at 100 / 60 / 30 %.
            foreground: rgba(0xd4d4d4ff),
            subtle: rgba(0xd4d4d499),
            subtlest: rgba(0xd4d4d44d),
            inverse: rgba(0x000000ff),
            brand: rgba(0xffffffff),
            accent: rgba(0x001d3dff),
            info: rgba(0x4099ffff),
            destructive: rgba(0xff5c5cff),
            success: rgba(0x46bf72ff),
            warning: rgba(0xff8a30ff),
            link: rgba(0x4099ffff),
            reasoning: rgba(0xa78bfaff),
            tool: rgba(0xf59e0bff),
            user: rgba(0x60a5faff),
            diff_add_bg: rgba(0x46bf7226),
            diff_del_bg: rgba(0xff5c5c26),
            diff_hunk_bg: rgba(0x4099ff1f),
            selection: rgba(0x4099ff47),
        }
    }

    pub fn zai_light() -> Self {
        Self {
            background: rgba(0xf8f8f8ff),
            sidebar: rgba(0xf0f0f0ff),
            panel: rgba(0xffffffff),
            card: rgba(0xffffffff),
            surface: rgba(0x0d0d0d08),
            surface_hover: rgba(0x0d0d0d0d),
            hover: rgba(0x0d0d0d0d),
            selected: rgba(0x0d0d0d0d),
            border: rgba(0x0d0d0d1a),
            border_hover: rgba(0x0d0d0d26),
            input: rgba(0xffffffff),
            menu: rgba(0xffffffff),
            menu_hover: rgba(0xf0f0f0ff),
            tag: rgba(0xe6e6e6ff),
            // neutral-800 at 100 / 60 / 40 %.
            foreground: rgba(0x262626ff),
            subtle: rgba(0x26262699),
            subtlest: rgba(0x26262666),
            inverse: rgba(0xffffffff),
            brand: rgba(0x000000ff),
            accent: rgba(0xebf4ffff),
            info: rgba(0x0b7fffff),
            destructive: rgba(0xe03131ff),
            success: rgba(0x1e8a3eff),
            warning: rgba(0xe07b00ff),
            link: rgba(0x0b7fffff),
            reasoning: rgba(0x7c3aedff),
            tool: rgba(0xd97706ff),
            user: rgba(0x2563ebff),
            diff_add_bg: rgba(0x1e8a3e1f),
            diff_del_bg: rgba(0xe031311f),
            diff_hunk_bg: rgba(0x0b7fff1a),
            selection: rgba(0x0b7fff33),
        }
    }

    /// Ely's semantic palette built from the same tokens, so Ely controls match the
    /// hand-drawn surfaces. Fields without a Desktop counterpart keep Ely's values.
    pub fn ely_palette(&self, light: bool) -> ely_gpui_component::theme::Palette {
        let mut out = if light {
            ely_gpui_component::theme::Palette::light(false)
        } else {
            ely_gpui_component::theme::Palette::dark(false)
        };
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
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zai_tokens_match_desktop_styles() {
        let dark = ThemePalette::zai_dark();
        assert_eq!(dark.background, rgba(0x161616ff));
        assert_eq!(dark.panel, rgba(0x202020ff));
        assert_eq!(dark.card, rgba(0x2b2b2bff));
        assert_eq!(dark.border, rgba(0xffffff1a));
        assert_eq!(dark.brand, rgba(0xffffffff));
        let light = ThemePalette::zai_light();
        assert_eq!(light.background, rgba(0xf8f8f8ff));
        assert_eq!(light.sidebar, rgba(0xf0f0f0ff));
        assert_eq!(light.border, rgba(0x0d0d0d1a));
        assert_eq!(light.brand, rgba(0x000000ff));
    }

    #[test]
    fn ely_primary_uses_brand_fill_with_inverse_text() {
        let dark = ThemePalette::zai_dark();
        let ely = dark.ely_palette(false);
        assert_eq!(ely.accent, Hsla::from(dark.brand));
        assert_eq!(ely.on_accent, Hsla::from(dark.inverse));
        assert_eq!(ely.bg, Hsla::from(dark.background));
    }
}
