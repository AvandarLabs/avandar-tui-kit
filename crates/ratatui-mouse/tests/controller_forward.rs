//! Press-time forwarding route and pane-relative gesture coordinates.

pub mod common;

use std::ops::RangeInclusive;

use common::{ControllerHarness, TestTarget};
use crossterm::event::MouseButton::{Left, Middle, Right};
use crossterm::event::MouseEventKind::{Down, Drag, Up};
use crossterm::event::{Event, KeyModifiers, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_mouse::{
    ContentPoint, ContentRange, LineSurface, MouseEncoding, MouseOutcome, MouseTracking,
    PaneMouseRequest, ScrollPosition, SelectableSurface, SurfaceSet, WordCharacters,
};

struct Pane<'a> {
    surface: LineSurface<'a, &'static str>,
    request: PaneMouseRequest,
}

impl SurfaceSet<TestTarget> for Pane<'_> {
    fn surface_mut(&mut self, target: &TestTarget) -> Option<&mut dyn SelectableSurface> {
        (*target == TestTarget::Left).then_some(self)
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
        Some(self.request)
    }
}

fn create_pane(scroll: &mut ScrollPosition) -> Pane<'_> {
    Pane {
        surface: LineSurface::new(&["alpha", "beta", "gamma", "delta"], scroll, 4),
        request: PaneMouseRequest {
            tracking: MouseTracking::AnyMotion,
            encoding: MouseEncoding::Sgr,
            is_alternate_screen: false,
            is_application_cursor: false,
        },
    }
}

fn assert_forward(outcome: &MouseOutcome<TestTarget>, expected: &[u8]) {
    assert_eq!(
        outcome,
        &MouseOutcome::Forward {
            target: TestTarget::Left,
            bytes: expected.to_vec(),
        }
    );
}

#[test]
fn forward_route_is_fixed_at_press() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&mut scroll));
    let press = harness.mouse(Down(Left), 12, 6, KeyModifiers::NONE);
    assert_eq!(press.pressed_target, Some(TestTarget::Left));
    assert_forward(&press.outcome, b"\x1b[<0;3;2M");
    assert_eq!(harness.controller.gesture_owner(), Some(&TestTarget::Left));
    harness.surfaces.request.tracking = MouseTracking::Off;
    harness.surfaces.request.encoding = MouseEncoding::Default;
    harness.controller.settings_mut().is_passthrough_enabled = false;
    assert!(!harness.controller.settings().is_passthrough_enabled);
    let drag = harness.mouse(Drag(Left), 35, 9, KeyModifiers::ALT);
    assert_forward(&drag.outcome, b"\x1b[<40;10;4M");
    let release = harness.mouse(Up(Left), 35, 9, KeyModifiers::SHIFT);
    assert_forward(&release.outcome, b"\x1b[<4;10;4m");
    assert!(release.pressed_target.is_none());
    assert!(harness.controller.selection().is_none());
    assert_eq!(harness.controller.gesture_owner(), None);
}

#[test]
fn bare_motion_is_ignored_with_and_without_an_active_forward_gesture() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&mut scroll));
    for is_pressed in [false, true] {
        if is_pressed {
            let _ = harness.mouse(Down(Right), 12, 6, KeyModifiers::NONE);
        }
        let report = harness.mouse(MouseEventKind::Moved, 35, 9, KeyModifiers::NONE);
        assert_eq!(report.outcome, MouseOutcome::Ignored);
        assert!(report.pressed_target.is_none());
        assert!(!report.is_redraw_due);
    }
    assert_eq!(harness.controller.gesture_owner(), Some(&TestTarget::Left));
}

#[test]
fn forward_uses_current_owner_area_and_drops_vanished_owners() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&mut scroll));
    let _ = harness.mouse(Down(Right), 12, 6, KeyModifiers::NONE);
    harness.areas.clear();
    harness.areas.push(Rect::new(2, 3, 4, 2), TestTarget::Left);
    let report = harness.mouse(Drag(Right), 35, 9, KeyModifiers::CONTROL);
    assert_forward(&report.outcome, b"\x1b[<50;4;2M");
    harness.areas.clear();
    let report = harness.event(&Event::FocusGained);
    assert!(matches!(
        report.outcome,
        MouseOutcome::Ignored | MouseOutcome::Consumed
    ));
    assert_eq!(harness.controller.gesture_owner(), None);
    let orphan = harness.mouse(Up(Right), 12, 6, KeyModifiers::NONE);
    assert!(matches!(
        orphan.outcome,
        MouseOutcome::Ignored | MouseOutcome::Consumed
    ));
}

#[test]
fn press_only_tracking_drops_release_without_leaving_a_gesture() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_pane(&mut scroll));
    harness.surfaces.request.tracking = MouseTracking::Press;
    let _ = harness.mouse(Down(Middle), 12, 6, KeyModifiers::NONE);
    let release = harness.mouse(Up(Middle), 12, 6, KeyModifiers::NONE);
    assert_eq!(release.outcome, MouseOutcome::Consumed);
    assert_eq!(harness.controller.gesture_owner(), None);
}
