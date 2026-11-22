//! Selection stages, expanded anchors, extension, copying, and cancellation.

pub mod common;

use common::{ControllerHarness, TestTarget};
use crossterm::event::KeyModifiers;
use crossterm::event::MouseButton::Left;
use crossterm::event::MouseEventKind::{Down, Drag, Up};
use ratatui::layout::Rect;
use ratatui_mouse::{
    ContentPoint, ContentRange, HighlightRects, LineSurface, MouseOutcome, ScrollPosition,
    SelectableSurface, SelectionStage, SurfaceSet,
};

struct TextSurfaces<'a> {
    surface: LineSurface<'a, &'static str>,
    is_present: bool,
}

impl SurfaceSet<TestTarget> for TextSurfaces<'_> {
    fn surface_mut(&mut self, target: &TestTarget) -> Option<&mut dyn SelectableSurface> {
        (self.is_present && *target == TestTarget::Left)
            .then_some(&mut self.surface as &mut dyn SelectableSurface)
    }
}

fn create_surfaces<'a>(
    rows: &'a [&'static str],
    scroll: &'a mut ScrollPosition,
) -> TextSurfaces<'a> {
    TextSurfaces {
        surface: LineSurface::new(rows, scroll, 4),
        is_present: true,
    }
}

fn selected_text(harness: &mut ControllerHarness<TextSurfaces<'_>>) -> Option<String> {
    harness.controller.selected_text(&mut harness.surfaces)
}

#[test]
fn selection_stages_distinguish_click_from_returning_drag() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    let press = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
    assert_eq!(press.pressed_target, Some(TestTarget::Left));
    assert!(matches!(press.outcome, MouseOutcome::Pressed { .. }));
    assert_eq!(
        harness
            .controller
            .selection()
            .map(ratatui_mouse::Selection::stage),
        Some(SelectionStage::Pressed)
    );
    assert_eq!(selected_text(&mut harness), None);
    let release = harness.mouse(Up(Left), 10, 5, KeyModifiers::NONE);
    assert!(matches!(release.outcome, MouseOutcome::Clicked { .. }));
    assert!(harness.controller.selection().is_none());
    harness.now += std::time::Duration::from_secs(1);
    let _ = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 12, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 10, 5, KeyModifiers::NONE);
    assert_eq!(
        harness.controller.selection().unwrap().stage(),
        SelectionStage::Dragging
    );
    let release = harness.mouse(Up(Left), 10, 5, KeyModifiers::NONE);
    assert!(matches!(release.outcome, MouseOutcome::Copy { ref text, .. } if text == "a"));
    assert_eq!(
        harness.controller.selection().unwrap().stage(),
        SelectionStage::Done
    );
}

#[test]
fn wide_endpoints_are_snapped_before_highlight_and_copy() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["a界b"], &mut scroll));
    let _ = harness.mouse(Down(Left), 12, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 11, 5, KeyModifiers::NONE);
    assert!(harness.controller.selection().is_some());
    let selection = harness.controller.selection().unwrap();
    assert_eq!(
        selection.range(),
        ContentRange::new(
            ContentPoint { line: 0, column: 1 },
            ContentPoint { line: 0, column: 2 }
        )
    );
    let rectangles: Vec<_> = HighlightRects::from_range(
        selection.range(),
        ContentPoint { line: 0, column: 0 },
        Rect::new(10, 5, 10, 4),
    )
    .as_slice()
    .to_vec();
    assert_eq!(rectangles, vec![Rect::new(11, 5, 2, 1)]);
    let release = harness.mouse(Up(Left), 12, 5, KeyModifiers::NONE);
    assert!(matches!(release.outcome, MouseOutcome::Copy { ref text, .. } if text == "界"));
}

fn press_count(
    harness: &mut ControllerHarness<TextSurfaces<'_>>,
    column: u16,
    row: u16,
    count: u8,
) {
    for _ in 1..count {
        let _ = harness.mouse(Down(Left), column, row, KeyModifiers::NONE);
        let _ = harness.mouse(Up(Left), column, row, KeyModifiers::NONE);
        harness.now += std::time::Duration::from_millis(10);
    }
    let _ = harness.mouse(Down(Left), column, row, KeyModifiers::NONE);
}

#[test]
fn multi_click_drag_preserves_anchor_unit() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["one /a.rs tail"], &mut scroll));
    harness.areas.clear();
    harness
        .areas
        .push(Rect::new(10, 5, 30, 4), TestTarget::Left);
    press_count(&mut harness, 16, 5, 2);
    assert_eq!(selected_text(&mut harness).as_deref(), Some("/a.rs"));
    assert_eq!(
        harness.controller.selection().unwrap().unit(),
        ratatui_mouse::SelectionUnit::Word
    );
    for (column, expected) in [(11, "one /a.rs"), (22, "/a.rs tail"), (16, "/a.rs")] {
        let _ = harness.mouse(Drag(Left), column, 5, KeyModifiers::NONE);
        assert_eq!(selected_text(&mut harness).as_deref(), Some(expected));
    }
    let report = harness.mouse(Up(Left), 16, 5, KeyModifiers::NONE);
    assert!(matches!(report.outcome, MouseOutcome::Copy { ref text, .. } if text == "/a.rs"));
    let report = harness.mouse(Down(Left), 16, 5, KeyModifiers::NONE);
    assert!(matches!(report.outcome, MouseOutcome::Pressed { press, .. } if press.clicks == 1));
}

#[test]
fn triple_click_drag_preserves_wrapped_logical_anchor() {
    for (anchor_row, endpoint_row, expected) in [
        (6, 8, "start middle\nlast\nend"),
        (7, 5, "start middle\nlast"),
    ] {
        let mut scroll = ScrollPosition::default();
        let mut surfaces = create_surfaces(&["start ", "middle ", "last", "end"], &mut scroll);
        surfaces.surface = surfaces
            .surface
            .with_wrapped_rows(&[true, false, false, false]);
        let mut harness = ControllerHarness::new(surfaces);
        press_count(&mut harness, 11, anchor_row, 3);
        assert_eq!(
            harness.controller.selection().unwrap().unit(),
            ratatui_mouse::SelectionUnit::Line
        );
        let _ = harness.mouse(Drag(Left), 11, endpoint_row, KeyModifiers::NONE);
        assert_eq!(selected_text(&mut harness).as_deref(), Some(expected));
        let report = harness.mouse(Up(Left), 11, endpoint_row, KeyModifiers::NONE);
        assert!(matches!(report.outcome, MouseOutcome::Copy { ref text, .. } if text == expected));
    }
}

#[test]
fn word_fallback_and_one_cell_click_drift() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["ab! cd"], &mut scroll));
    let _ = harness.mouse(Down(Left), 11, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Up(Left), 11, 5, KeyModifiers::NONE);
    let press = harness.mouse(Down(Left), 12, 5, KeyModifiers::NONE);
    assert!(matches!(press.outcome, MouseOutcome::Pressed { press, .. } if press.clicks == 2));
    assert_eq!(selected_text(&mut harness).as_deref(), Some("!"));
    let report = harness.mouse(Up(Left), 12, 5, KeyModifiers::NONE);
    assert!(matches!(report.outcome, MouseOutcome::Copy { ref text, .. } if text == "!"));
}

fn finish_cell_drag(harness: &mut ControllerHarness<TextSurfaces<'_>>, start: u16, end: u16) {
    let _ = harness.mouse(Down(Left), start, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), end, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Up(Left), end, 5, KeyModifiers::NONE);
}

#[test]
fn modifier_extension_and_copy_settings() {
    for modifier in [KeyModifiers::SHIFT, KeyModifiers::ALT] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abcdefghij"], &mut scroll));
        finish_cell_drag(&mut harness, 15, 12);
        let report = harness.mouse(Down(Left), 17, 5, modifier);
        assert_eq!(report.pressed_target, Some(TestTarget::Left));
        assert_eq!(
            harness.controller.selection().unwrap().stage(),
            SelectionStage::Pressed
        );
        assert_eq!(selected_text(&mut harness).as_deref(), Some("fgh"));
        let release = harness.mouse(Up(Left), 17, 5, modifier);
        assert!(matches!(release.outcome, MouseOutcome::Copy { ref text, .. } if text == "fgh"));
        let _ = harness.mouse(Down(Left), 11, 5, modifier);
        assert_eq!(selected_text(&mut harness).as_deref(), Some("bcdef"));
        let _ = harness.mouse(Up(Left), 11, 5, modifier);
        let _ = harness.mouse(Down(Left), 30, 5, modifier);
        assert!(harness.controller.selection().is_none());
    }
}

#[test]
fn copy_and_keep_flags_apply_independently() {
    for (should_copy, should_keep) in [(false, true), (false, false), (true, false)] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
        harness.controller.settings_mut().should_copy_on_release = should_copy;
        harness
            .controller
            .settings_mut()
            .should_keep_selection_after_copy = should_keep;
        let _ = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
        let _ = harness.mouse(Drag(Left), 11, 5, KeyModifiers::NONE);
        let report = harness.mouse(Up(Left), 11, 5, KeyModifiers::NONE);
        assert_eq!(
            matches!(report.outcome, MouseOutcome::Copy { .. }),
            should_copy
        );
        assert_eq!(
            harness.controller.selection().is_some(),
            !should_copy || should_keep
        );
        if !should_copy {
            assert_eq!(selected_text(&mut harness).as_deref(), Some("ab"));
        }
    }
}

#[test]
fn blank_only_finished_selection_is_dropped() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["  \t  "], &mut scroll));
    let _ = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 11, 5, KeyModifiers::NONE);
    let report = harness.mouse(Up(Left), 11, 5, KeyModifiers::NONE);
    assert!(!matches!(report.outcome, MouseOutcome::Copy { .. }));
    assert!(harness.controller.selection().is_none());
}

#[test]
fn key_press_and_resize_clearing_obey_flags() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, ModifierKeyCode};
    for should_clear in [true, false] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
        harness.controller.settings_mut().should_clear_on_key_press = should_clear;
        harness.controller.settings_mut().should_clear_on_resize = should_clear;
        finish_cell_drag(&mut harness, 10, 11);
        for key in [
            KeyEvent::new_with_kind(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            ),
            KeyEvent::new(
                KeyCode::Modifier(ModifierKeyCode::LeftShift),
                KeyModifiers::SHIFT,
            ),
        ] {
            let _ = harness.event(&Event::Key(key));
            assert_eq!(selected_text(&mut harness).as_deref(), Some("ab"));
        }
        let _ = harness.event(&Event::Key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
        )));
        assert_eq!(harness.controller.selection().is_none(), should_clear);
        harness.now += std::time::Duration::from_secs(1);
        finish_cell_drag(&mut harness, 10, 11);
        let _ = harness.event(&Event::Resize(80, 24));
        assert_eq!(harness.controller.selection().is_none(), should_clear);
    }
}

#[test]
fn modifier_extension_keeps_word_unit_and_original_anchor() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["one two"], &mut scroll));
    press_count(&mut harness, 15, 5, 2);
    let _ = harness.mouse(Up(Left), 15, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Down(Left), 11, 5, KeyModifiers::ALT);
    assert_eq!(selected_text(&mut harness).as_deref(), Some("one two"));
    assert_eq!(
        harness.controller.selection().unwrap().unit(),
        ratatui_mouse::SelectionUnit::Word
    );
    let _ = harness.mouse(Drag(Left), 16, 5, KeyModifiers::NONE);
    assert_eq!(selected_text(&mut harness).as_deref(), Some("two"));
}

#[test]
fn selection_cancellation_never_copies() {
    use crossterm::event::{Event, MouseButton::Right};
    for cancellation in 0..7 {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abcdef"], &mut scroll));
        let _ = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
        let _ = harness.mouse(Drag(Left), 12, 5, KeyModifiers::NONE);
        let report = match cancellation {
            0 => harness.event(&Event::FocusLost),
            1 => harness.event(&Event::Resize(80, 24)),
            2 => {
                assert!(harness.controller.cancel_gesture());
                harness.event(&Event::FocusGained)
            }
            3 => {
                harness.areas.clear();
                harness.event(&Event::FocusGained)
            }
            4 => harness.mouse(Down(Right), 30, 5, KeyModifiers::NONE),
            5 => {
                harness.surfaces.is_present = false;
                harness.event(&Event::FocusGained)
            }
            _ => {
                assert!(harness.controller.forget_target(&TestTarget::Left));
                harness.event(&Event::FocusGained)
            }
        };
        assert!(!matches!(report.outcome, MouseOutcome::Copy { .. }));
        assert!(
            harness.controller.selection().is_none(),
            "cancellation {cancellation}"
        );
        let orphan = harness.mouse(Up(Left), 12, 5, KeyModifiers::NONE);
        assert!(!matches!(orphan.outcome, MouseOutcome::Copy { .. }));
    }
}

#[test]
fn completed_selection_is_cleared_when_target_or_surface_disappears() {
    use crossterm::event::Event;
    for is_missing_area in [true, false] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
        finish_cell_drag(&mut harness, 10, 11);
        if is_missing_area {
            harness.areas.clear();
        } else {
            harness.surfaces.is_present = false;
        }
        let report = harness.event(&Event::FocusGained);
        assert!(!matches!(report.outcome, MouseOutcome::Copy { .. }));
        assert!(harness.controller.selection().is_none());
    }
}

#[test]
fn changed_owner_area_clamps_selection_and_ignores_unowned_events() {
    use crossterm::event::{MouseButton::Right, MouseEventKind::Moved};
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abcdef"], &mut scroll));
    let _ = harness.mouse(Down(Left), 11, 5, KeyModifiers::NONE);
    for event in [Moved, Drag(Right), Up(Right)] {
        let report = harness.mouse(event, 35, 8, KeyModifiers::NONE);
        assert_eq!(report.outcome, MouseOutcome::Ignored);
        assert_eq!(
            harness.controller.selection().unwrap().stage(),
            SelectionStage::Pressed
        );
    }
    harness.areas.clear();
    harness.areas.push(Rect::new(2, 3, 3, 1), TestTarget::Left);
    harness
        .areas
        .push(Rect::new(30, 5, 10, 4), TestTarget::Right);
    let _ = harness.mouse(Drag(Left), 35, 8, KeyModifiers::NONE);
    assert_eq!(
        harness.controller.selection().unwrap().target(),
        &TestTarget::Left
    );
    assert_eq!(selected_text(&mut harness).as_deref(), Some("bc"));
    let rectangles = HighlightRects::from_range(
        harness.controller.selection().unwrap().range(),
        ContentPoint { line: 0, column: 0 },
        Rect::new(2, 3, 3, 1),
    );
    assert_eq!(rectangles.as_slice(), &[Rect::new(3, 3, 2, 1)]);
    let report = harness.mouse(Up(Left), 35, 8, KeyModifiers::NONE);
    assert!(
        matches!(report.outcome, MouseOutcome::Copy { target: TestTarget::Left, ref text } if text == "bc")
    );
}

#[test]
fn clearing_active_selection_cancels_its_pending_release() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    let _ = harness.mouse(Down(Left), 10, 5, KeyModifiers::NONE);
    let _ = harness.mouse(Drag(Left), 11, 5, KeyModifiers::NONE);
    assert!(harness.controller.clear_selection());
    assert!(harness.controller.gesture_owner().is_none());
    let release = harness.mouse(Up(Left), 11, 5, KeyModifiers::NONE);
    assert_eq!(release.outcome, MouseOutcome::Ignored);
}
