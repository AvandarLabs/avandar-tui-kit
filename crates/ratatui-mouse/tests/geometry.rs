//! Frame geometry, content coordinates and inclusive ranges.

use ratatui::layout::{Position, Rect};
use ratatui_mouse::{ContentPoint, ContentRange, HitAreas, clamp_position_into};

#[derive(Debug, PartialEq)]
struct Target(String);

#[test]
fn topmost_hit_clamp_and_content_roundtrip() {
    let mut areas = HitAreas::new();
    areas.push(Rect::new(0, 0, 40, 20), Target("pane".into()));
    areas.push(Rect::new(10, 5, 20, 8), Target("modal".into()));
    assert_eq!(
        areas.target_at(Position::new(12, 6)),
        Some(&Target("modal".into()))
    );
    assert_eq!(
        areas.hit_at(Position::new(12, 6)).map(|(area, _)| area),
        Some(Rect::new(10, 5, 20, 8))
    );
}

#[test]
fn empty_areas_are_ignored_and_latest_target_area_wins() {
    let mut areas = HitAreas::new();
    areas.push(Rect::new(1, 1, 0, 5), Target("empty".into()));
    areas.push(Rect::new(1, 1, 5, 0), Target("empty".into()));
    assert!(areas.is_empty());
    areas.push(Rect::new(2, 2, 3, 3), Target("pane".into()));
    areas.push(Rect::new(8, 8, 2, 2), Target("pane".into()));
    assert_eq!(
        areas.area_of(&Target("pane".into())),
        Some(Rect::new(8, 8, 2, 2))
    );
    assert_eq!(
        areas.iter().map(|(area, _)| area).collect::<Vec<_>>(),
        [Rect::new(2, 2, 3, 3), Rect::new(8, 8, 2, 2)]
    );
    assert!(areas.target_at(Position::new(5, 2)).is_none());
    assert!(areas.target_at(Position::new(2, 5)).is_none());
    areas.clear();
    assert!(areas.is_empty());
    assert!(areas.target_at(Position::new(2, 2)).is_none());
}

#[test]
fn clamp_reports_signed_distance_and_nearest_cell() {
    let area = Rect::new(10, 5, 20, 8);
    for (pointer, position, overshoot) in [
        (Position::new(3, 2), Position::new(10, 5), -3),
        (Position::new(60, 20), Position::new(29, 12), 8),
        (Position::new(17, 8), Position::new(17, 8), 0),
    ] {
        let clamped = clamp_position_into(area, pointer, 0).unwrap();
        assert_eq!(clamped.position, position);
        assert_eq!(clamped.overshoot_rows, overshoot);
    }
}

#[test]
fn margins_count_outer_rows_and_resolve_overlap_by_nearest_edge() {
    let area = Rect::new(10, 5, 20, 8);
    for (row, overshoot) in [(5, -2), (6, -1), (7, 0), (10, 0), (11, 1), (12, 2)] {
        let clamped = clamp_position_into(area, Position::new(12, row), 2).unwrap();
        assert_eq!(clamped.position, Position::new(12, row));
        assert_eq!(clamped.overshoot_rows, overshoot);
    }
    for (height, row, margin, overshoot) in [
        (2, 5, 1, -1),
        (2, 6, 1, 1),
        (3, 6, 3, -2),
        (3, 7, u16::MAX, 3),
    ] {
        let clamped =
            clamp_position_into(Rect::new(10, 5, 2, height), Position::new(10, row), margin)
                .unwrap();
        assert_eq!(clamped.overshoot_rows, overshoot);
        assert_eq!(clamped.position, Position::new(10, row));
    }
}

#[test]
fn content_roundtrip_tracks_both_origin_axes() {
    let area = Rect::new(10, 5, 20, 8);
    let origin = point(-12, 40);
    let content = ContentPoint::from_screen_position(Position::new(17, 8), area, origin);
    assert_eq!(content, point(-9, 47));
    assert_eq!(
        content.to_screen_position(area, origin),
        Some(Position::new(17, 8))
    );
    for point in [
        point(-13, 40),
        point(-4, 40),
        point(-12, 39),
        point(-12, 60),
    ] {
        assert_eq!(point.to_screen_position(area, origin), None);
    }
}

#[test]
fn inclusive_ranges_normalize_contain_and_union_both_ends() {
    let first = point(-2, 8);
    let last = point(3, 2);
    let range = ContentRange::new(last, first);
    assert_eq!(range.start, first);
    assert_eq!(range.end, last);
    for point in [first, last, point(0, 999)] {
        assert!(range.contains(point));
    }
    assert!(!range.contains(point(-2, 7)));
    assert!(!range.contains(point(3, 3)));
    let union = range.union(ContentRange::new(last, point(5, 0)));
    assert_eq!(union.start, first);
    assert_eq!(union.end, point(5, 0));
    assert_eq!(ContentRange::new(last, last).union(range), range);
}

#[test]
fn empty_and_extreme_geometry_does_not_overflow() {
    for area in [Rect::new(10, 5, 0, 8), Rect::new(10, 5, 8, 0)] {
        assert!(clamp_position_into(area, Position::new(0, 0), u16::MAX).is_none());
        let origin = point(i64::MIN, u32::MAX);
        assert_eq!(origin.to_screen_position(area, origin), None);
    }
    let area = Rect {
        x: u16::MAX - 2,
        y: u16::MAX - 2,
        width: 20,
        height: 20,
    };
    let clamped = clamp_position_into(area, Position::new(u16::MAX, u16::MAX), u16::MAX).unwrap();
    assert_eq!(clamped.position, Position::new(u16::MAX - 1, u16::MAX - 1));
    let origin = point(i64::MAX - 1, u32::MAX - 1);
    assert_eq!(
        ContentPoint::from_screen_position(Position::new(u16::MAX - 1, u16::MAX - 1), area, origin),
        point(i64::MAX, u32::MAX)
    );
    assert_eq!(
        (point(i64::MAX, u32::MAX)).to_screen_position(area, origin),
        Some(Position::new(u16::MAX - 1, u16::MAX - 1))
    );
    assert_eq!(
        (point(i64::MAX, 0)).to_screen_position(Rect::new(0, 0, 3, 3), point(i64::MIN, 0)),
        None
    );
    assert_eq!(
        ContentPoint::from_screen_position(Position::new(2, 2), Rect::new(0, 0, 3, 3), origin),
        point(i64::MAX, u32::MAX)
    );
}

const fn point(line: i64, column: u32) -> ContentPoint {
    ContentPoint { line, column }
}
