//! Wheel routing, injected-time scrolling, and redraw deadlines.

pub mod common;

use std::{ops::RangeInclusive, time::Duration};

use common::{ControllerHarness, TestTarget};
use crossterm::event::MouseEventKind::{
    Down, Drag, ScrollDown, ScrollLeft, ScrollRight, ScrollUp, Up,
};
use crossterm::event::{Event, KeyModifiers, MouseButton::Left};
use ratatui::layout::Rect;
use ratatui_mouse::{
    ContentPoint, ContentRange, LineSurface, MouseEncoding, MouseOutcome, MouseReport,
    MouseTracking, NoSurfaces, PaneMouseRequest, ScrollPosition, SelectableSurface, SurfaceSet,
    WordCharacters,
};

struct Pane<'a> {
    surface: LineSurface<'a, &'static str>,
    request: Option<PaneMouseRequest>,
    is_present: bool,
}

impl SurfaceSet<TestTarget> for Pane<'_> {
    fn surface_mut(&mut self, target: &TestTarget) -> Option<&mut dyn SelectableSurface> {
        (self.is_present && *target == TestTarget::Right)
            .then_some(self as &mut dyn SelectableSurface)
    }
}

impl SelectableSurface for Pane<'_> {
    fn origin(&self) -> ContentPoint {
        self.surface.origin()
    }
    fn line_bounds(&self) -> RangeInclusive<i64> {
        self.surface.line_bounds()
    }
    fn character_at(&mut self, point: ContentPoint) -> ContentRange {
        self.surface.character_at(point)
    }
    fn word_at(
        &mut self,
        point: ContentPoint,
        characters: &WordCharacters,
    ) -> Option<ContentRange> {
        self.surface.word_at(point, characters)
    }
    fn line_at(&mut self, line: i64) -> ContentRange {
        self.surface.line_at(line)
    }
    fn text(&mut self, range: ContentRange) -> String {
        self.surface.text(range)
    }
    fn scroll_by(&mut self, rows: i32) -> i32 {
        self.surface.scroll_by(rows)
    }
    fn pane_request(&self) -> Option<PaneMouseRequest> {
        self.request
    }
}

fn create_pane<'a>(rows: &'a [&'static str], scroll: &'a mut ScrollPosition) -> Pane<'a> {
    Pane {
        surface: LineSurface::new(rows, scroll, 4),
        request: None,
        is_present: true,
    }
}

const fn create_request(tracking: MouseTracking) -> PaneMouseRequest {
    PaneMouseRequest {
        tracking,
        encoding: MouseEncoding::Sgr,
        is_alternate_screen: false,
        is_application_cursor: false,
    }
}

fn tick<S: SurfaceSet<TestTarget>>(harness: &mut ControllerHarness<S>) -> MouseReport<TestTarget> {
    harness
        .controller
        .tick(harness.now, &harness.areas, &mut harness.surfaces)
}

fn drag_outside(harness: &mut ControllerHarness<Pane<'_>>) {
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 31, 28, KeyModifiers::NONE);
}

#[test]
fn edge_ticks_scroll_without_new_events() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    drag_outside(&mut harness);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
    assert_eq!(harness.surfaces.origin().line, 0);
    for _ in 0..20 {
        let _ = harness.mouse(Drag(Left), 31, 28, KeyModifiers::NONE);
    }
    assert_eq!(harness.surfaces.origin().line, 0);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 10);
    assert_eq!(harness.controller.selection().unwrap().range().end.line, 13);
    harness.now += Duration::from_millis(16);
    let _ = tick(&mut harness);
    assert_eq!(
        harness.controller.next_deadline(),
        Some(harness.now + Duration::from_millis(34))
    );
    harness.now += Duration::from_millis(10);
    let _ = harness.mouse(Drag(Left), 31, 9, KeyModifiers::NONE);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 10);
    harness.now += Duration::from_millis(24);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 11);
    harness.now += Duration::from_secs(5);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 12);
    assert_eq!(
        harness.controller.next_deadline(),
        Some(harness.now + Duration::from_millis(50))
    );
}

#[test]
fn edges_scroll_up_and_stop_at_content_boundary() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition { line: 2, column: 0 };
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 31, 3, KeyModifiers::NONE);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 0);
    harness.now += Duration::from_millis(50);
    let _ = tick(&mut harness);
    assert_eq!(harness.controller.next_deadline(), None);
    let _ = harness.mouse(Drag(Left), 31, 3, KeyModifiers::NONE);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
}

#[test]
fn returning_inside_stops_edge_work() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    drag_outside(&mut harness);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
    let _ = harness.mouse(Drag(Left), 31, 6, KeyModifiers::NONE);
    harness.now += Duration::from_millis(16);
    let _ = tick(&mut harness);
    assert_eq!(harness.controller.next_deadline(), None);
    assert_eq!(harness.surfaces.origin().line, 0);
}

#[test]
fn cancellation_clears_edge_work_on_events_and_ticks() {
    let rows = ["text"; 100];
    for cancellation in 0..9 {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
        drag_outside(&mut harness);
        assert_eq!(harness.controller.next_deadline(), Some(harness.now));
        match cancellation {
            0 => {
                let _ = harness.mouse(Up(Left), 31, 28, KeyModifiers::NONE);
            }
            1 => {
                let _ = harness.event(&Event::Resize(80, 24));
            }
            2 => {
                let _ = harness.event(&Event::FocusLost);
            }
            3 => {
                assert!(harness.controller.cancel_gesture());
            }
            4 => {
                assert!(harness.controller.clear_selection());
            }
            5 => {
                assert!(harness.controller.forget_target(&TestTarget::Right));
            }
            6 => {
                harness.areas.clear();
            }
            7 => {
                harness.surfaces.is_present = false;
            }
            _ => {
                let _ = harness.mouse(Down(Left), 11, 6, KeyModifiers::NONE);
            }
        }
        harness.now += Duration::from_millis(16);
        let _ = tick(&mut harness);
        assert_eq!(
            harness.controller.next_deadline(),
            None,
            "case {cancellation}"
        );
        assert_eq!(harness.surfaces.origin().line, 0, "case {cancellation}");
    }
}

#[test]
fn wheel_reports_hovered_target_and_preserves_all_directions() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    for (kind, rows, columns) in [
        (ScrollUp, -3, 0),
        (ScrollDown, 3, 0),
        (ScrollLeft, 0, -3),
        (ScrollRight, 0, 3),
    ] {
        let report = harness.mouse(kind, 31, 6, KeyModifiers::CONTROL);
        assert_eq!(
            report.outcome,
            MouseOutcome::Wheel {
                target: TestTarget::Right,
                rows,
                columns,
                modifiers: KeyModifiers::CONTROL
            }
        );
        assert_eq!(report.pressed_target, None);
    }
    assert_eq!(
        harness.mouse(ScrollDown, 0, 0, KeyModifiers::NONE).outcome,
        MouseOutcome::Ignored
    );
    let _ = harness.mouse(Down(Left), 11, 6, KeyModifiers::NONE);
    assert!(matches!(
        harness.mouse(ScrollDown, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Wheel {
            target: TestTarget::Right,
            rows: 3,
            columns: 0,
            ..
        }
    ));
}

#[test]
fn wheel_during_selection_updates_content_end() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    let report = harness.mouse(ScrollDown, 11, 7, KeyModifiers::NONE);
    assert_eq!(report.outcome, MouseOutcome::Consumed);
    assert_eq!(harness.surfaces.origin().line, 3);
    assert_eq!(
        harness.controller.selection().unwrap().range().end,
        ContentPoint { line: 5, column: 0 }
    );
    let report = harness.mouse(ScrollRight, 31, 6, KeyModifiers::NONE);
    assert_eq!(report.outcome, MouseOutcome::Consumed);
    assert_eq!(harness.surfaces.origin().line, 3);
    assert_eq!(
        harness.controller.selection().unwrap().range().end,
        ContentPoint { line: 4, column: 1 }
    );
    let _ = harness.mouse(ScrollUp, 31, 6, KeyModifiers::NONE);
    assert_eq!(harness.surfaces.origin().line, 0);
    let _ = harness.mouse(Up(Left), 31, 6, KeyModifiers::NONE);
    let range = harness.controller.selection().unwrap().range();
    assert!(matches!(
        harness.mouse(ScrollDown, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Wheel { rows: 3, .. }
    ));
    assert_eq!(harness.controller.selection().unwrap().range(), range);
    assert_eq!(harness.surfaces.origin().line, 0);
}

#[test]
fn pane_wheel_routing_and_forward_gesture_snapshot() {
    let rows = ["text"; 10];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    harness.surfaces.request = Some(create_request(MouseTracking::ButtonMotion));
    for (kind, expected) in [
        (ScrollUp, b"\x1b[<64;2;2M".as_slice()),
        (ScrollDown, b"\x1b[<65;2;2M"),
        (ScrollLeft, b"\x1b[<66;2;2M"),
        (ScrollRight, b"\x1b[<67;2;2M"),
    ] {
        assert_eq!(
            harness.mouse(kind, 31, 6, KeyModifiers::NONE).outcome,
            MouseOutcome::Forward {
                target: TestTarget::Right,
                bytes: expected.to_vec()
            }
        );
    }
    assert!(matches!(
        harness
            .mouse(ScrollDown, 31, 6, KeyModifiers::SHIFT)
            .outcome,
        MouseOutcome::Wheel { rows: 3, .. }
    ));
    harness.controller.settings_mut().is_passthrough_enabled = false;
    assert!(matches!(
        harness.mouse(ScrollDown, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Wheel { rows: 3, .. }
    ));
    harness.controller.settings_mut().is_passthrough_enabled = true;
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    harness.surfaces.request = Some(create_request(MouseTracking::Off));
    let report = harness.mouse(ScrollDown, 11, 9, KeyModifiers::NONE);
    assert_eq!(
        report.outcome,
        MouseOutcome::Forward {
            target: TestTarget::Right,
            bytes: b"\x1b[<65;1;4M".to_vec()
        }
    );
}

#[test]
fn alternate_screen_wheel_uses_vertical_arrow_keys_only() {
    let rows = ["text"; 10];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let mut request = create_request(MouseTracking::Off);
    request.is_alternate_screen = true;
    harness.surfaces.request = Some(request);
    assert_eq!(
        harness.mouse(ScrollDown, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Forward {
            target: TestTarget::Right,
            bytes: b"\x1b[B\x1b[B\x1b[B".to_vec()
        }
    );
    request.is_application_cursor = true;
    harness.surfaces.request = Some(request);
    assert_eq!(
        harness.mouse(ScrollUp, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Forward {
            target: TestTarget::Right,
            bytes: b"\x1bOA\x1bOA\x1bOA".to_vec()
        }
    );
    assert!(matches!(
        harness.mouse(ScrollLeft, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Wheel {
            rows: 0,
            columns: -3,
            ..
        }
    ));
    harness.controller.settings_mut().is_wheel_to_arrows_enabled = false;
    assert!(matches!(
        harness.mouse(ScrollDown, 31, 6, KeyModifiers::NONE).outcome,
        MouseOutcome::Wheel { rows: 3, .. }
    ));
}

#[test]
fn redraw_budget_delivers_pending_change_at_deadline() {
    let rows = ["text"; 10];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let start = harness.now;
    assert!(
        harness
            .mouse(Down(Left), 31, 6, KeyModifiers::NONE)
            .is_redraw_due
    );
    harness.now += Duration::from_millis(1);
    assert!(
        !harness
            .mouse(Drag(Left), 32, 6, KeyModifiers::NONE)
            .is_redraw_due
    );
    assert_eq!(
        harness.controller.next_deadline(),
        Some(start + Duration::from_millis(16))
    );
    assert!(!tick(&mut harness).is_redraw_due);
    harness.now = start + Duration::from_millis(16);
    assert!(tick(&mut harness).is_redraw_due);
    assert_eq!(harness.controller.next_deadline(), None);
    assert!(!tick(&mut harness).is_redraw_due);
    assert!(
        !harness
            .mouse(Drag(Left), 32, 6, KeyModifiers::NONE)
            .is_redraw_due
    );
    assert_eq!(harness.controller.next_deadline(), None);
    harness.now += Duration::from_millis(1);
    assert!(!harness.event(&Event::FocusLost).is_redraw_due);
    assert_eq!(
        harness.controller.next_deadline(),
        Some(start + Duration::from_millis(32))
    );
    harness.now = start + Duration::from_millis(32);
    assert!(tick(&mut harness).is_redraw_due);
}

#[test]
fn direct_selection_clear_is_redrawn_on_next_tick() {
    let rows = ["text"; 10];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    assert!(harness.controller.clear_selection());
    assert_eq!(
        harness.controller.next_deadline(),
        Some(harness.now + Duration::from_millis(16))
    );
    harness.now += Duration::from_millis(16);
    assert!(tick(&mut harness).is_redraw_due);
}

#[test]
fn zero_intervals_maxima_and_tiny_margin_are_bounded() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition {
        line: 20,
        column: 0,
    };
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    harness.areas.clear();
    harness
        .areas
        .push(Rect::new(30, 5, 1, 1), TestTarget::Right);
    let settings = harness.controller.settings_mut();
    settings.redraw_interval = Duration::ZERO;
    settings.auto_scroll_interval = Duration::ZERO;
    settings.auto_scroll_max_rows = 0;
    settings.edge_margin_rows = u16::MAX;
    let _ = harness.mouse(Down(Left), 30, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 30, 5, KeyModifiers::NONE);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 20);
    assert_eq!(harness.controller.next_deadline(), None);
    harness.controller.settings_mut().auto_scroll_max_rows = u16::MAX;
    let _ = harness.mouse(Drag(Left), 30, 5, KeyModifiers::NONE);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 19);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
    let _ = harness.mouse(Drag(Left), u16::MAX, u16::MAX, KeyModifiers::NONE);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 96);
}

#[test]
fn overflowing_intervals_do_not_panic() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    harness.controller.settings_mut().auto_scroll_interval = Duration::MAX;
    harness.controller.settings_mut().redraw_interval = Duration::MAX;
    drag_outside(&mut harness);
    assert_eq!(harness.controller.next_deadline(), Some(harness.now));
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 10);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 10);
}

#[test]
fn partial_boundary_scroll_clears_future_edge_work() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition { line: 2, column: 0 };
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    let _ = harness.mouse(Down(Left), 31, 6, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 31, 0, KeyModifiers::NONE);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 0);
    harness.now += Duration::from_millis(16);
    let _ = tick(&mut harness);
    assert_eq!(harness.controller.next_deadline(), None);
}

#[test]
fn wheel_inside_owner_clears_stale_edge_work() {
    let rows = ["text"; 100];
    for (kind, expected_origin) in [(ScrollDown, 3), (ScrollRight, 0)] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
        drag_outside(&mut harness);
        assert_eq!(harness.controller.next_deadline(), Some(harness.now));
        let report = harness.mouse(kind, 31, 6, KeyModifiers::NONE);
        assert_eq!(report.outcome, MouseOutcome::Consumed);
        assert_eq!(harness.surfaces.origin().line, expected_origin);
        let _ = tick(&mut harness);
        assert_eq!(harness.surfaces.origin().line, expected_origin);
        harness.now += Duration::from_millis(16);
        let _ = tick(&mut harness);
        assert_eq!(harness.controller.next_deadline(), None);
        harness.now += Duration::from_millis(50);
        let _ = tick(&mut harness);
        assert_eq!(harness.surfaces.origin().line, expected_origin);
    }
}

#[test]
fn wheel_outside_refreshes_overshoot_and_preserves_edge_deadline() {
    let rows = ["text"; 100];
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&rows, &mut scroll));
    drag_outside(&mut harness);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 10);
    let start = harness.now;
    harness.now += Duration::from_millis(10);
    let report = harness.mouse(ScrollDown, 31, 9, KeyModifiers::NONE);
    assert_eq!(report.outcome, MouseOutcome::Consumed);
    assert_eq!(harness.surfaces.origin().line, 13);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 13);
    harness.now = start + Duration::from_millis(16);
    let _ = tick(&mut harness);
    assert_eq!(
        harness.controller.next_deadline(),
        Some(start + Duration::from_millis(50))
    );
    harness.now = start + Duration::from_millis(50);
    let _ = tick(&mut harness);
    assert_eq!(harness.surfaces.origin().line, 14);
    assert_eq!(harness.controller.selection().unwrap().range().end.line, 17);
}
