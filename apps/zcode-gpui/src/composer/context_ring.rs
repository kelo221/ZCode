//! Composer context-window ring with its usage hover card.
//! Desktop parity: packages/ui/src/components/ai-elements/context.tsx (ring)
//! and chat-input-toolbar/contextUsage.tsx (card).

use crate::conversation::context_usage::{ContextWindow, percent};
use crate::shared::i18n::{Locale, current_locale, label};
use crate::shared::theme::{MONO_FONT, active_theme, ui_size};
use ely_gpui_component::overlays::HoverCard;
use gpui::{
    Bounds, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Point, Rgba, Styled, Window,
    canvas, div, point, prelude::*, px, relative,
};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Desktop's 24-unit SVG at 14px: radius 10 and stroke 4.
const RING: f32 = 14.;
const RADIUS: f32 = RING * 10. / 24.;
const STROKE: f32 = RING * 4. / 24.;
const STEPS: usize = 64;
/// Shares of the breakdown bar, mixed from the chart color toward the surface.
const TONES: [f32; 5] = [1., 0.78, 0.58, 0.42, 0.28];

pub(crate) fn context_ring(cw: ContextWindow) -> impl IntoElement {
    let fraction = cw.fraction() as f32;
    let hover = active_theme().hover;
    let trigger = div()
        .id("chat-context-usage-trigger")
        .size(px(28.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_lg()
        .hover(move |s| s.bg(hover))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| paint_ring(bounds, fraction, window),
            )
            .size(px(RING)),
        );
    HoverCard::new("chat-context-usage", trigger, move |_, _| card(&cw))
}

fn paint_ring(bounds: Bounds<Pixels>, fraction: f32, window: &mut Window) {
    let subtle: Hsla = active_theme().subtle.into();
    let c = bounds.center();
    let at = |r: f32, a: f32| c + point(px(r * a.cos()), px(r * a.sin()));
    let mut track = PathBuilder::stroke(px(STROKE));
    track.add_polygon(&circle(|a| at(RADIUS, a), -FRAC_PI_2, TAU), true);
    paint(track, subtle.opacity(0.25), window);
    if fraction <= 0. {
        return;
    }
    if fraction >= 0.999 {
        let mut full = PathBuilder::stroke(px(STROKE));
        full.add_polygon(&circle(|a| at(RADIUS, a), -FRAC_PI_2, TAU), true);
        paint(full, subtle.opacity(0.7), window);
        return;
    }
    // One filled outline (outer edge, round end cap, inner edge, round start
    // cap): separate cap dots would double the alpha where they overlap.
    let (h, start, sweep) = (STROKE / 2., -FRAC_PI_2, TAU * fraction);
    let end = start + sweep;
    let (tip, tail) = (at(RADIUS, end), at(RADIUS, start));
    let cap = |centre: Point<Pixels>, from: f32| {
        circle(
            move |a| centre + point(px(h * a.cos()), px(h * a.sin())),
            from,
            PI,
        )
    };
    let mut outline = circle(|a| at(RADIUS + h, a), start, sweep);
    outline.extend(cap(tip, end));
    outline.extend(circle(|a| at(RADIUS - h, a), end, -sweep));
    outline.extend(cap(tail, start + PI));
    let mut arc = PathBuilder::fill();
    arc.add_polygon(&outline, true);
    paint(arc, subtle.opacity(0.7), window);
}

/// Points along `sweep` radians from `from` (clockwise on screen when positive).
fn circle(point_at: impl Fn(f32) -> Point<Pixels>, from: f32, sweep: f32) -> Vec<Point<Pixels>> {
    let steps = ((STEPS as f32 * sweep.abs() / TAU).ceil() as usize).max(2);
    (0..=steps)
        .map(|i| point_at(from + sweep * i as f32 / steps as f32))
        .collect()
}

fn paint(path: PathBuilder, color: Hsla, window: &mut Window) {
    // Only a degenerate outline fails to tessellate; it has nothing to draw.
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn card(cw: &ContextWindow) -> impl IntoElement + use<> {
    let p = active_theme();
    let zh = current_locale() == Locale::ZhCn;
    let chart = chart_color();
    let tone = |i: usize| mix(chart, p.surface, TONES[i.min(TONES.len() - 1)]);
    let small = px(ui_size(12.));

    let bar_fill = div()
        .h_full()
        .min_w(px(8.))
        .w(relative(cw.fraction() as f32))
        .rounded_full()
        .overflow_hidden()
        .flex()
        .map(|el| match cw.segments.is_empty() {
            true => el.bg(p.brand),
            false => el.children(cw.segments.iter().enumerate().map(|(i, s)| {
                div()
                    .h_full()
                    .flex_none()
                    .w(relative(s.share as f32))
                    .bg(tone(i))
            })),
        });
    let rows = cw.segments.iter().enumerate().map(|(i, s)| {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(small)
            .child(
                div()
                    .size(px(8.))
                    .flex_none()
                    .rounded_sm()
                    .border_1()
                    .border_color(p.border)
                    .bg(tone(i)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.subtle)
                    .child(label(s.label_en, s.label_zh)),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .font_family(MONO_FONT)
                    .text_color(p.foreground)
                    .child(percent(s.share)),
            )
    });
    let cache = cw.cache_label().map(|rate| {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .text_size(small)
            .when(!cw.segments.is_empty(), |el| {
                el.border_t_1().border_color(p.border).pt_3()
            })
            .child(
                div()
                    .text_color(p.subtle)
                    .child(label("Average cache hit rate", "平均缓存命中率")),
            )
            .child(
                div()
                    .font_family(MONO_FONT)
                    .text_color(p.foreground)
                    .child(rate),
            )
    });

    div()
        .id("chat-context-usage-card")
        .w(px(320.))
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .mb_1()
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(ui_size(13.)))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(p.foreground)
                                .child(label("Context windows", "上下文容量")),
                        )
                        .child(
                            div()
                                .ml_auto()
                                .flex_none()
                                .font_family(MONO_FONT)
                                .text_size(small)
                                .text_color(p.subtle)
                                .child(cw.summary(zh)),
                        ),
                )
                .child(
                    div()
                        .h(px(8.))
                        .w_full()
                        .rounded_md()
                        .bg(p.surface)
                        .flex()
                        .child(bar_fill),
                ),
        )
        .when(!cw.segments.is_empty(), |el| {
            el.child(div().flex().flex_col().gap(px(6.)).children(rows))
        })
        .children(cache)
}

/// Desktop's `--color-usage-chart-1`, mapped to the palette's brand purple.
fn chart_color() -> Rgba {
    active_theme().brand
}

/// `amount` of `a` mixed with the rest of `b`.
fn mix(a: Rgba, b: Rgba, amount: f32) -> Rgba {
    let m = |x: f32, y: f32| x * amount + y * (1. - amount);
    Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: 1.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::rgb;

    #[test]
    fn arc_runs_clockwise_from_twelve_o_clock() {
        let c = point(px(7.), px(7.));
        let pts = circle(
            |a| c + point(px(5. * a.cos()), px(5. * a.sin())),
            -FRAC_PI_2,
            TAU / 4.,
        );
        let (first, last) = (pts[0], *pts.last().unwrap());
        assert!(
            (first.x - px(7.)).abs() < px(0.01) && (first.y - px(2.)).abs() < px(0.01),
            "top"
        );
        assert!(
            (last.x - px(12.)).abs() < px(0.01) && (last.y - px(7.)).abs() < px(0.01),
            "right"
        );
    }

    #[test]
    fn tones_fade_toward_the_surface() {
        let (a, b) = (rgb(0x0000ff), rgb(0xffffff));
        assert_eq!(mix(a, b, 1.), a);
        let half = mix(a, b, 0.5);
        assert!((half.r - 0.5).abs() < 1e-6 && half.b == 1.);
    }
}
