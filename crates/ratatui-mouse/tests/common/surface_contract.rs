//! Shared fixture recipe for both built-in adapters and app-defined surfaces.

use ratatui_mouse::{ContentPoint, ContentRange, SelectableSurface, WordCharacters};

pub const ROWS: [&str; 5] = ["alpha", "beta  ", "a界b", "wrappe", "d  "];
pub const WRAPPED_ROWS: [bool; 5] = [false, false, false, true, false];
pub const WIDTH: u16 = 6;
pub const VISIBLE_ROWS: u16 = 2;

pub const fn point(line: i64, column: u32) -> ContentPoint {
    ContentPoint { line, column }
}

pub fn assert_surface_contract(surface: &mut dyn SelectableSurface) {
    assert_eq!(surface.line_bounds(), 0..=4);
    let origin = surface.origin();
    assert_ne!(
        origin.line, 0,
        "fixture must begin scrolled away from start"
    );
    assert_copy_contract(surface, origin);
    assert_scroll_contract(surface, origin);
}

fn assert_copy_contract(surface: &mut dyn SelectableSurface, origin: ContentPoint) {
    assert_eq!(
        surface.text(ContentRange::new(
            point(0, 0),
            point(4, u32::from(WIDTH - 1))
        )),
        "alpha\nbeta\na界b\nwrapped"
    );
    assert_eq!(
        surface.origin(),
        origin,
        "copy must preserve both origin axes"
    );
    assert_eq!(
        surface.text(ContentRange::new(point(2, 2), point(2, 2))),
        "界"
    );
    assert_eq!(
        surface.character_at(point(2, 2)),
        ContentRange::new(point(2, 1), point(2, 2))
    );
    assert_eq!(
        surface.word_at(point(2, 2), &WordCharacters::default()),
        Some(ContentRange::new(point(2, 0), point(2, 3)))
    );
    assert_eq!(
        surface.word_at(point(1, 5), &WordCharacters::default()),
        None
    );
    for line in [3, 4] {
        let range = surface.line_at(line);
        assert_eq!(range.start, point(3, 0));
        assert_eq!(range.end.line, 4);
        assert_eq!(surface.text(range), "wrapped");
    }
}

fn assert_scroll_contract(surface: &mut dyn SelectableSurface, origin: ContentPoint) {
    assert_eq!(
        surface.scroll_by(i32::MIN),
        -i32::try_from(origin.line).unwrap()
    );
    assert_eq!(surface.origin(), point(0, origin.column));
    assert_eq!(surface.scroll_by(-1), 0);
    assert_eq!(surface.scroll_by(i32::MAX), 3);
    assert_eq!(surface.origin(), point(3, origin.column));
    assert_eq!(surface.scroll_by(1), 0);
    assert_eq!(surface.scroll_by(-2), -2);
    assert_eq!(surface.origin(), point(1, origin.column));
}
