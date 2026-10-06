//! Compatibility bridge for legacy token-based renderers.

use crate::shared::theme::*;

pub(crate) fn color(token: u32) -> gpui::Rgba {
    let palette = active_theme();
    let resolved = match token {
        BG => palette.bg,
        PANEL => palette.panel,
        CARD => palette.card,
        CARD_HOVER => palette.card_hover,
        BORDER => palette.border,
        HOVER => palette.hover,
        SELECTED => palette.selected,
        TEXT => palette.text,
        MUTED => palette.muted,
        ACCENT => palette.accent,
        USER_BLUE => palette.user_blue,
        REASONING => palette.reasoning,
        TOOL => palette.tool,
        DANGER => palette.danger,
        SUCCESS => palette.success,
        CODE_BG => palette.code_bg,
        CODE_BORDER => palette.code_border,
        CODE_HEADER => palette.code_header,
        DIFF_ADD_BG => palette.diff_add_bg,
        DIFF_DEL_BG => palette.diff_del_bg,
        DIFF_HUNK_BG => palette.diff_hunk_bg,
        _ => token,
    };
    gpui::rgb(resolved)
}
