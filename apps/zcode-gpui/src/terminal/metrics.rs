use gpui::{App, px};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

impl Default for CellMetrics {
    fn default() -> Self {
        Self {
            width: crate::terminal::pane::CELL_W,
            height: crate::terminal::pane::CELL_H,
        }
    }
}

pub(crate) fn font_metrics(font_size: f32, cx: &App) -> CellMetrics {
    let text = cx.text_system();
    let font = text.resolve_font(&gpui::font(crate::shared::theme::MONO_FONT));
    let fallback = CellMetrics::default();
    let width = text
        .em_advance(font, px(font_size))
        .ok()
        .map(|advance| f64::from(advance) as f32)
        .filter(|width| width.is_finite() && *width > 0.)
        .unwrap_or(fallback.width);
    let natural =
        f64::from(text.ascent(font, px(font_size)) + text.descent(font, px(font_size))) as f32;
    let height = if natural.is_finite() && natural > 0. {
        natural.ceil().max(font_size * 1.3)
    } else {
        fallback.height
    };
    CellMetrics { width, height }
}
