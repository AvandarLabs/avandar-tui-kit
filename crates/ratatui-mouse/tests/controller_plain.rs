//! Ownership and cancellation without selectable surfaces.

pub mod common;

use std::time::Duration;

use common::{ControllerHarness, TestTarget};
use crossterm::event::MouseButton::{Left, Right};
use crossterm::event::MouseEventKind::{Down, Drag, Up};
use crossterm::event::{Event, KeyModifiers, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui_mouse::{MouseOutcome, NoSurfaces};

const LEFT_DOWN: MouseEventKind = Down(Left);
const LEFT_UP: MouseEventKind = Up(Left);

#[test]
fn plain_gesture_keeps_press_owner() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    let report = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    assert_eq!(report.pressed_target, Some(TestTarget::Left));
    assert!(
        matches!(report.outcome, MouseOutcome::Pressed { target: TestTarget::Left, press } if press.position == Position::new(12, 6) && press.clicks == 1 && press.content_point.is_none())
    );
    assert_eq!(harness.controller.gesture_owner(), Some(&TestTarget::Left));
    let report = harness.mouse(Drag(Left), 35, 9, KeyModifiers::ALT);
    assert_eq!(
        report.outcome,
        MouseOutcome::Dragged {
            target: TestTarget::Left,
            button: Left,
            from: Position::new(12, 6),
            to: Position::new(19, 8)
        }
    );
    let report = harness.mouse(LEFT_UP, 35, 9, KeyModifiers::NONE);
    assert_eq!(
        report.outcome,
        MouseOutcome::DragEnded {
            target: TestTarget::Left,
            button: Left,
            from: Position::new(12, 6),
            to: Position::new(19, 8)
        }
    );
    assert_eq!(harness.controller.gesture_owner(), None);
}

#[test]
fn click_keeps_original_press_and_click_runs_reset_on_drag() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    for (offset, clicks) in [(0, 1), (10, 2)] {
        harness.now += Duration::from_millis(offset);
        let press = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::CONTROL);
        let release = harness.mouse(LEFT_UP, 35, 9, KeyModifiers::NONE);
        assert!(
            matches!(release.outcome, MouseOutcome::Clicked { target: TestTarget::Left, press } if press.clicks == clicks && press.modifiers == KeyModifiers::CONTROL && press.position == Position::new(12, 6))
        );
        assert!(press.pressed_target.is_some());
    }
    let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 12, 6, KeyModifiers::NONE);
    let _ = harness.mouse(LEFT_UP, 12, 6, KeyModifiers::NONE);
    let report = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    assert!(matches!(report.outcome, MouseOutcome::Pressed { press, .. } if press.clicks == 1));
}

#[test]
fn lost_release_and_missing_owner_cancel_without_copy() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    let orphan = harness.mouse(LEFT_UP, 12, 6, KeyModifiers::NONE);
    assert!(matches!(
        orphan.outcome,
        MouseOutcome::Ignored | MouseOutcome::Consumed
    ));
    let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    let replacement = harness.mouse(LEFT_DOWN, 32, 6, KeyModifiers::NONE);
    assert!(matches!(
        replacement.outcome,
        MouseOutcome::Pressed {
            target: TestTarget::Right,
            ..
        }
    ));
    assert_eq!(harness.controller.gesture_owner(), Some(&TestTarget::Right));
    harness.areas.clear();
    let report = harness.mouse(LEFT_UP, 12, 6, KeyModifiers::NONE);
    assert!(matches!(
        report.outcome,
        MouseOutcome::Ignored | MouseOutcome::Consumed
    ));
    assert_eq!(harness.controller.gesture_owner(), None);
}

#[test]
fn focus_resize_forgetting_and_explicit_cancel_never_finish_gestures() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    for event in [Event::FocusLost, Event::Resize(80, 24)] {
        let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
        let report = harness.event(&event);
        assert!(matches!(
            report.outcome,
            MouseOutcome::Ignored | MouseOutcome::Consumed
        ));
        assert_eq!(harness.controller.gesture_owner(), None);
    }
    let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    assert!(!harness.controller.forget_target(&TestTarget::Right));
    assert!(harness.controller.forget_target(&TestTarget::Left));
    assert!(!harness.controller.forget_target(&TestTarget::Left));
    let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    assert!(harness.controller.cancel_gesture());
    assert!(!harness.controller.cancel_gesture());
    assert!(harness.controller.selection().is_none());
    assert_eq!(harness.controller.selected_text(&mut NoSurfaces), None);
    assert!(!harness.controller.clear_selection());
}

#[test]
fn current_area_controls_clamping_and_nonowner_buttons_are_ignored() {
    let mut harness = ControllerHarness::new(NoSurfaces);
    let _ = harness.mouse(LEFT_DOWN, 12, 6, KeyModifiers::NONE);
    let report = harness.mouse(Up(Right), 12, 6, KeyModifiers::NONE);
    assert!(matches!(
        report.outcome,
        MouseOutcome::Ignored | MouseOutcome::Consumed
    ));
    assert_eq!(harness.controller.gesture_owner(), Some(&TestTarget::Left));
    harness.areas.clear();
    harness.areas.push(Rect::new(1, 1, 3, 2), TestTarget::Left);
    let report = harness.mouse(Drag(Left), 35, 9, KeyModifiers::NONE);
    assert!(
        matches!(report.outcome, MouseOutcome::Dragged { from, to, .. } if from == Position::new(12, 6) && to == Position::new(3, 2))
    );
    let report = harness.mouse(LEFT_DOWN, 40, 40, KeyModifiers::NONE);
    assert!(report.pressed_target.is_none());
    assert_eq!(harness.controller.gesture_owner(), None);
}
