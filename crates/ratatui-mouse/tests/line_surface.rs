//! Borrowed app-held row adapter regressions.

pub mod common;

use common::surface_contract::{ROWS, VISIBLE_ROWS, WRAPPED_ROWS, assert_surface_contract, point};
use ratatui_mouse::{
    ContentRange, LineSurface, NoSurfaces, ScrollPosition, SelectableSurface, SurfaceSet,
    WordCharacters,
};

#[test]
fn borrowed_rows_follow_shared_surface_contract() {
    let mut scroll = ScrollPosition { line: 1, column: 1 };
    let mut surface =
        LineSurface::new(&ROWS, &mut scroll, VISIBLE_ROWS).with_wrapped_rows(&WRAPPED_ROWS);
    assert_surface_contract(&mut surface);
    assert_eq!(surface.pane_request(), None);
    assert_eq!(
        surface.line_at(4),
        ContentRange::new(point(3, 0), point(4, 2))
    );
    assert_eq!(scroll, ScrollPosition { line: 1, column: 1 });
}

#[test]
fn missing_wrap_entries_and_empty_rows_are_safe() {
    let rows = ["ab ", "cd", "", "ef"];
    let mut scroll = ScrollPosition::default();
    let mut surface = LineSurface::new(&rows, &mut scroll, 2).with_wrapped_rows(&[true]);
    assert_eq!(
        surface.text(ContentRange::new(point(0, 0), point(3, 8))),
        "ab cd\n\nef"
    );
    assert_eq!(
        surface.line_at(1),
        ContentRange::new(point(0, 0), point(1, 1))
    );
    assert_eq!(
        surface.line_at(2),
        ContentRange::new(point(2, 0), point(2, 0))
    );
    assert_eq!(
        surface.character_at(point(2, 4)),
        ContentRange::new(point(2, 4), point(2, 4))
    );
    assert_eq!(
        surface.word_at(point(-1, 0), &WordCharacters::default()),
        None
    );
    assert_eq!(
        surface.text(ContentRange::new(point(-2, 9), point(0, 1))),
        "ab"
    );
    assert_eq!(
        surface.text(ContentRange::new(point(8, 0), point(9, 0))),
        ""
    );
}

#[test]
fn empty_surface_and_oversized_viewports_do_not_scroll() {
    let rows: [&str; 0] = [];
    let mut scroll = ScrollPosition::default();
    let mut surface = LineSurface::new(&rows, &mut scroll, 0);
    assert!(surface.line_bounds().is_empty());
    assert_eq!(
        surface.text(ContentRange::new(point(0, 0), point(9, 9))),
        ""
    );
    assert_eq!(surface.scroll_by(i32::MAX), 0);
    assert_eq!(
        surface.line_at(-1),
        ContentRange::new(point(-1, 0), point(-1, 0))
    );
    let rows = [String::from("alpha")];
    let mut surface = LineSurface::new(&rows, &mut scroll, 20);
    assert_eq!(surface.scroll_by(2), 0);
    assert_eq!(
        surface.text(ContentRange::new(point(0, 0), point(0, 20))),
        "alpha"
    );
    assert!(NoSurfaces.surface_mut(&"target").is_none());
}
