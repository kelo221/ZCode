use super::*;
use gpui::{point, px, size};

#[test]
fn terminal_selection_geometry_uses_content_bounds_side_and_history_offset() {
    let bounds = Bounds::new(point(px(10.), px(20.)), size(px(132.), px(45.)));
    let dims = crate::terminal::pane::TermDims { cols: 20, rows: 3 };
    let (point, side) =
        cell_at(bounds, point(px(24.), px(38.)), dims, 2, Default::default()).unwrap();
    assert_eq!(point, TermPoint::new(Line(-1), Column(2)));
    assert_eq!(side, Side::Left);
    let (point, side) = cell_at(
        bounds,
        gpui::point(px(28.), px(38.)),
        dims,
        2,
        Default::default(),
    )
    .unwrap();
    assert_eq!(point, TermPoint::new(Line(-1), Column(2)));
    assert_eq!(side, Side::Right);
    let (point, _) = cell_at(
        bounds,
        gpui::point(px(-100.), px(1000.)),
        dims,
        0,
        Default::default(),
    )
    .unwrap();
    assert_eq!(point, TermPoint::new(Line(2), Column(0)));
    let (point, _) = cell_at(
        bounds,
        gpui::point(px(31.), px(44.)),
        dims,
        0,
        CellMetrics {
            width: 10.,
            height: 20.,
        },
    )
    .unwrap();
    assert_eq!(point, TermPoint::new(Line(1), Column(2)));
    assert!(
        cell_at(
            bounds,
            bounds.origin,
            dims,
            0,
            CellMetrics {
                width: 0.,
                height: 20.
            }
        )
        .is_none()
    );
    assert!(
        cell_at(
            Bounds::default(),
            bounds.origin,
            dims,
            0,
            Default::default()
        )
        .is_none()
    );
    assert!(
        cell_at(
            bounds,
            gpui::point(px(f32::NAN), px(20.)),
            dims,
            0,
            Default::default()
        )
        .is_none()
    );
}

#[test]
fn terminal_selection_copy_preserves_wrapping_wide_combining_and_scrollback() {
    use alacritty_terminal::{
        event::VoidListener,
        grid::Scroll,
        term::{Config, Term},
        vte::ansi::Processor,
    };
    let dims = crate::terminal::pane::TermDims { cols: 4, rows: 3 };
    let mut term = Term::new(Config::default(), &dims, VoidListener);
    let mut processor: Processor = Processor::default();
    processor.advance(&mut term, "ab中e\u{301}f\r\nlast".as_bytes());
    let mut selection = Selection::new(
        SelectionType::Simple,
        TermPoint::new(Line(0), Column(0)),
        Side::Left,
    );
    selection.update(TermPoint::new(Line(1), Column(1)), Side::Right);
    term.selection = Some(selection);
    assert_eq!(term.selection_to_string().as_deref(), Some("ab中e\u{301}f"));
    processor.advance(&mut term, b"\r\nnext\r\nmore");
    term.scroll_display(Scroll::Top);
    let content = term.renderable_content();
    let offset = content.display_offset;
    assert!(offset > 0);
    let mut selection = Selection::new(
        SelectionType::Simple,
        TermPoint::new(Line(-(offset as i32)), Column(0)),
        Side::Left,
    );
    selection.update(
        TermPoint::new(Line(-(offset as i32)), Column(3)),
        Side::Right,
    );
    term.selection = Some(selection);
    assert_eq!(term.selection_to_string().as_deref(), Some("ab中"));
}
