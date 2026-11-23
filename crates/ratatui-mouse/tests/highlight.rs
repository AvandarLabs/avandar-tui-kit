//! Inclusive selection geometry and non-destructive buffer styling.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use ratatui_mouse::{ContentPoint, ContentRange, DEFAULT_HIGHLIGHT_STYLE, HighlightRects};

fn range(start: (i64, u32), end: (i64, u32)) -> ContentRange {
    ContentRange::new(
        ContentPoint {
            line: start.0,
            column: start.1,
        },
        ContentPoint {
            line: end.0,
            column: end.1,
        },
    )
}

fn create_highlight_cases() -> Vec<(ContentRange, Vec<Rect>)> {
    vec![
        (range((21, 5), (21, 7)), vec![Rect::new(11, 6, 3, 1)]),
        (
            range((20, 6), (24, 8)),
            vec![
                Rect::new(12, 5, 6, 1),
                Rect::new(10, 6, 8, 3),
                Rect::new(10, 9, 5, 1),
            ],
        ),
        (
            range((20, 6), (21, 8)),
            vec![Rect::new(12, 5, 6, 1), Rect::new(10, 6, 5, 1)],
        ),
        (range((21, 0), (21, 6)), vec![Rect::new(10, 6, 3, 1)]),
        (range((21, 9), (21, 99)), vec![Rect::new(15, 6, 3, 1)]),
        (
            range((19, 6), (22, 8)),
            vec![Rect::new(10, 5, 8, 2), Rect::new(10, 7, 5, 1)],
        ),
        (
            range((22, 6), (25, 8)),
            vec![Rect::new(12, 7, 6, 1), Rect::new(10, 8, 8, 2)],
        ),
        (range((0, 0), (19, 99)), vec![]),
        (range((25, 0), (30, 99)), vec![]),
        (range((21, 0), (21, 3)), vec![]),
        (range((21, 12), (21, 99)), vec![]),
    ]
}

#[test]
fn highlight_clips_to_at_most_three_rectangles() {
    let origin = ContentPoint {
        line: 20,
        column: 4,
    };
    let area = Rect::new(10, 5, 8, 5);
    for (selection, expected) in create_highlight_cases() {
        let rectangles = HighlightRects::from_range(selection, origin, area);
        assert_eq!(rectangles.as_slice(), expected, "{selection:?}");
    }
    for empty_area in [Rect::new(10, 5, 0, 5), Rect::new(10, 5, 8, 0)] {
        assert_eq!(
            HighlightRects::from_range(range((20, 4), (24, 11)), origin, empty_area).as_slice(),
            []
        );
    }
}

#[test]
fn highlight_handles_extreme_content_and_screen_bounds() {
    let rectangles = HighlightRects::from_range(
        range((i64::MIN, 0), (i64::MAX, u32::MAX)),
        ContentPoint {
            line: i64::MAX - 1,
            column: u32::MAX - 1,
        },
        Rect::new(u16::MAX - 3, u16::MAX - 3, 8, 8),
    );
    assert_eq!(
        rectangles.as_slice(),
        &[
            Rect::new(u16::MAX - 3, u16::MAX - 3, 2, 1),
            Rect::new(u16::MAX - 3, u16::MAX - 2, 2, 1),
        ]
    );
    let lowest = HighlightRects::from_range(
        range((i64::MIN, 0), (i64::MIN, 0)),
        ContentPoint {
            line: i64::MIN,
            column: 0,
        },
        Rect::new(3, 4, 1, 1),
    );
    assert_eq!(lowest.as_slice(), &[Rect::new(3, 4, 1, 1)]);
    for area in [Rect::new(u16::MAX, 1, 2, 2), Rect::new(1, u16::MAX, 2, 2)] {
        assert_eq!(
            HighlightRects::from_range(
                range((0, 0), (2, 2)),
                ContentPoint { line: 0, column: 0 },
                area
            )
            .as_slice(),
            []
        );
    }
}

#[test]
fn highlight_patches_style_without_replacing_text() {
    let area = Rect::new(10, 5, 8, 3);
    let base_style = Style::new()
        .fg(Color::Green)
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD);
    let mut buffer = Buffer::empty(area);
    buffer.set_string(10, 5, "a界bcd", base_style);
    let original = buffer.clone();
    let rectangles = HighlightRects::from_range(
        range((0, 1), (0, 2)),
        ContentPoint { line: 0, column: 0 },
        area,
    );
    assert_eq!(rectangles.as_slice(), &[Rect::new(11, 5, 2, 1)]);
    rectangles.render(&mut buffer, DEFAULT_HIGHLIGHT_STYLE);
    for position in area.positions() {
        let before = &original[position];
        let after = &buffer[position];
        assert_eq!(after.symbol(), before.symbol());
        let expected = if position.y == 5 && (11..=12).contains(&position.x) {
            before.style().add_modifier(Modifier::REVERSED)
        } else {
            before.style()
        };
        assert_eq!(after.style(), expected, "{position:?}");
    }
    rectangles.render(
        &mut buffer,
        Style::new()
            .fg(Color::Yellow)
            .remove_modifier(Modifier::BOLD),
    );
    assert_eq!(buffer[(11, 5)].fg, Color::Yellow);
    assert_eq!(buffer[(11, 5)].bg, Color::Blue);
    assert!(buffer[(11, 5)].modifier.contains(Modifier::REVERSED));
    assert!(!buffer[(11, 5)].modifier.contains(Modifier::BOLD));
}

#[test]
fn highlight_render_clips_to_the_buffer() {
    let rectangles = HighlightRects::from_range(
        range((0, 0), (2, 7)),
        ContentPoint { line: 0, column: 0 },
        Rect::new(10, 5, 8, 3),
    );
    let mut buffer = Buffer::filled(Rect::new(12, 6, 2, 1), ratatui::buffer::Cell::new("x"));
    rectangles.render(&mut buffer, DEFAULT_HIGHLIGHT_STYLE);
    assert_eq!(buffer[(12, 6)].symbol(), "x");
    assert!(buffer[(12, 6)].modifier.contains(Modifier::REVERSED));
    assert!(buffer[(13, 6)].modifier.contains(Modifier::REVERSED));
}
