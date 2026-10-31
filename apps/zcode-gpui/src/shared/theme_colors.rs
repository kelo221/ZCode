//! Compatibility bridge: legacy token identities to the GCode (Dracula) semantic colors.

use crate::shared::theme::*;

pub(crate) fn color(token: u32) -> gpui::Rgba {
    let p = active_theme();
    match token {
        BG => p.background,
        PANEL => p.panel,
        CARD => p.card,
        CARD_HOVER => p.menu_hover,
        BORDER => p.border,
        HOVER => p.hover,
        SELECTED => p.selected,
        // TEXT and PRIMARY share an identity.
        TEXT => p.foreground,
        MUTED => p.subtle,
        ACCENT => p.brand,
        // USER_BLUE and LINK share an identity.
        USER_BLUE => p.link,
        REASONING => p.reasoning,
        // TOOL, AMBER and WARNING share an identity.
        TOOL => p.warning,
        DANGER => p.destructive,
        SUCCESS => p.success,
        CODE_BG => p.background,
        CODE_BORDER => p.border,
        CODE_HEADER => p.surface,
        DIFF_ADD_BG => p.diff_add_bg,
        DIFF_DEL_BG => p.diff_del_bg,
        DIFF_HUNK_BG => p.diff_hunk_bg,
        SIDEBAR => p.sidebar,
        SURFACE => p.surface,
        INPUT => p.input,
        TAG => p.tag,
        SUBTLEST => p.subtlest,
        BORDER_HOVER => p.border_hover,
        INVERSE => p.inverse,
        // Desktop 只在 Windows/Linux 使用不透明的窗口底色；其他平台回落到侧栏色。
        FRAME if cfg!(target_os = "macos") => p.sidebar,
        FRAME => p.window_frame,
        _ => gpui::rgb(token),
    }
}

/// A token at reduced alpha, for Desktop-style translucent badges and banners
/// (`bg-<color>/15` and similar). Multiplies, so translucent tokens such as
/// the border stay translucent (`border-border/50`).
pub(crate) fn tinted(token: u32, alpha: f32) -> gpui::Rgba {
    let c = color(token);
    gpui::Rgba {
        a: c.a * alpha,
        ..c
    }
}
