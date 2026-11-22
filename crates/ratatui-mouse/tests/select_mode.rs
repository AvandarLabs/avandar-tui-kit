//! Keyboard-only selection and interception over borrowed real text surfaces.

pub mod common;

use common::{ControllerHarness, TestTarget};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui_mouse::{
    ContentPoint, LineSurface, MouseOutcome, MouseReport, PaneMouseRequest, ScrollPosition,
    SelectableSurface, SelectionCommand, SelectionMotion, SelectionUnit, SurfaceSet,
    selection_command_from_key,
};

struct TextSurfaces<'a> {
    surface: LineSurface<'a, &'static str>,
    is_present: bool,
    request: Option<PaneMouseRequest>,
}

impl SurfaceSet<TestTarget> for TextSurfaces<'_> {
    fn surface_mut(&mut self, target: &TestTarget) -> Option<&mut dyn SelectableSurface> {
        (self.is_present && *target == TestTarget::Left)
            .then_some(self as &mut dyn SelectableSurface)
    }
}

impl SelectableSurface for TextSurfaces<'_> {
    fn origin(&self) -> ContentPoint {
        self.surface.origin()
    }
    fn line_bounds(&self) -> std::ops::RangeInclusive<i64> {
        self.surface.line_bounds()
    }
    fn character_at(&mut self, point: ContentPoint) -> ratatui_mouse::ContentRange {
        self.surface.character_at(point)
    }
    fn word_at(
        &mut self,
        point: ContentPoint,
        words: &ratatui_mouse::WordCharacters,
    ) -> Option<ratatui_mouse::ContentRange> {
        self.surface.word_at(point, words)
    }
    fn line_at(&mut self, line: i64) -> ratatui_mouse::ContentRange {
        self.surface.line_at(line)
    }
    fn text(&mut self, range: ratatui_mouse::ContentRange) -> String {
        self.surface.text(range)
    }
    fn scroll_by(&mut self, rows: i32) -> i32 {
        self.surface.scroll_by(rows)
    }
    fn pane_request(&self) -> Option<PaneMouseRequest> {
        self.request
    }
}

fn create_surfaces<'a>(
    rows: &'a [&'static str],
    scroll: &'a mut ScrollPosition,
) -> TextSurfaces<'a> {
    TextSurfaces {
        surface: LineSurface::new(rows, scroll, 4),
        is_present: true,
        request: None,
    }
}

fn enter(
    harness: &mut ControllerHarness<TextSurfaces<'_>>,
    start: Option<ContentPoint>,
) -> MouseReport<TestTarget> {
    harness.controller.enter_select_mode(
        TestTarget::Left,
        start,
        &harness.areas,
        &mut harness.surfaces,
    )
}

fn command(
    harness: &mut ControllerHarness<TextSurfaces<'_>>,
    command: SelectionCommand,
) -> MouseReport<TestTarget> {
    harness
        .controller
        .apply_command(command, harness.now, &harness.areas, &mut harness.surfaces)
}

fn key(
    harness: &mut ControllerHarness<TextSurfaces<'_>>,
    code: KeyCode,
) -> MouseReport<TestTarget> {
    harness.event(&Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

const fn create_default_bindings() -> [(KeyCode, SelectionCommand); 26] {
    use SelectionCommand::{
        Cancel, Copy, Move, SelectWord, ToggleCellSelection, ToggleLineSelection,
    };
    use SelectionMotion::{
        Down, FirstLine, LastLine, Left, LineEnd, LineStart, PageDown, PageUp, Right, Up,
        WordBackward, WordForward,
    };
    [
        (KeyCode::Char('h'), Move(Left)),
        (KeyCode::Left, Move(Left)),
        (KeyCode::Char('j'), Move(Down)),
        (KeyCode::Down, Move(Down)),
        (KeyCode::Char('k'), Move(Up)),
        (KeyCode::Up, Move(Up)),
        (KeyCode::Char('l'), Move(Right)),
        (KeyCode::Right, Move(Right)),
        (KeyCode::Char('w'), Move(WordForward)),
        (KeyCode::Char('b'), Move(WordBackward)),
        (KeyCode::Char('0'), Move(LineStart)),
        (KeyCode::Home, Move(LineStart)),
        (KeyCode::Char('$'), Move(LineEnd)),
        (KeyCode::End, Move(LineEnd)),
        (KeyCode::PageUp, Move(PageUp)),
        (KeyCode::PageDown, Move(PageDown)),
        (KeyCode::Char('g'), Move(FirstLine)),
        (KeyCode::Char('G'), Move(LastLine)),
        (KeyCode::Char('v'), ToggleCellSelection),
        (KeyCode::Char(' '), ToggleCellSelection),
        (KeyCode::Char('V'), ToggleLineSelection),
        (KeyCode::Char('W'), SelectWord),
        (KeyCode::Char('y'), Copy),
        (KeyCode::Enter, Copy),
        (KeyCode::Esc, Cancel),
        (KeyCode::Char('q'), Cancel),
    ]
}

#[test]
fn keymap_covers_the_spec_and_ignores_releases() {
    for (code, expected) in create_default_bindings() {
        for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
            assert_eq!(
                selection_command_from_key(&KeyEvent::new_with_kind(
                    code,
                    KeyModifiers::NONE,
                    kind
                )),
                Some(expected)
            );
        }
        assert_eq!(
            selection_command_from_key(&KeyEvent::new_with_kind(
                code,
                KeyModifiers::NONE,
                KeyEventKind::Release
            )),
            None
        );
    }
}

#[test]
fn control_and_shift_bindings_accept_only_supported_modifiers() {
    use SelectionCommand::{Move, SelectWord, ToggleLineSelection};
    use SelectionMotion::{HalfPageDown, HalfPageUp, LastLine, LineEnd};
    for (character, expected) in [('u', HalfPageUp), ('d', HalfPageDown)] {
        assert_eq!(
            selection_command_from_key(&KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL
            )),
            Some(Move(expected))
        );
    }
    for (character, expected) in [
        ('G', Move(LastLine)),
        ('V', ToggleLineSelection),
        ('W', SelectWord),
        ('$', Move(LineEnd)),
    ] {
        assert_eq!(
            selection_command_from_key(&KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::SHIFT
            )),
            Some(expected)
        );
    }
    for (code, modifiers) in [
        (KeyCode::Char('z'), KeyModifiers::NONE),
        (KeyCode::Char('h'), KeyModifiers::ALT),
        (KeyCode::Char('q'), KeyModifiers::CONTROL),
    ] {
        assert_eq!(
            selection_command_from_key(&KeyEvent::new(code, modifiers)),
            None
        );
    }
}

#[test]
fn keyboard_only_selection_scrolls_and_copies() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(
        &["one two", "a界", "tail", "last row", "five", "six"],
        &mut scroll,
    ));
    harness.surfaces.surface = harness.surfaces.surface.with_wrapped_rows(&[false, true]);
    assert!(enter(&mut harness, None).is_redraw_due);
    assert_eq!(
        harness.controller.select_mode_cursor(),
        Some(ContentPoint { line: 3, column: 0 })
    );
    let _ = key(&mut harness, KeyCode::Char('v'));
    let _ = key(&mut harness, KeyCode::Char('l'));
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("la")
    );
    let _ = key(&mut harness, KeyCode::Char('v'));
    let _ = key(&mut harness, KeyCode::Char('l'));
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("la")
    );
    let _ = key(&mut harness, KeyCode::Char('q'));
    assert!(harness.controller.selection().is_none());
    assert!(harness.controller.select_mode_target().is_some());
    let _ = key(&mut harness, KeyCode::Char('q'));
    assert!(harness.controller.select_mode_target().is_none());
}

#[test]
fn keyboard_line_and_word_copy_preserve_original_wrapped_text() {
    let mut scroll = ScrollPosition::default();
    let mut harness =
        ControllerHarness::new(create_surfaces(&["one two", "a界", "tail"], &mut scroll));
    harness.surfaces.surface = harness.surfaces.surface.with_wrapped_rows(&[false, true]);
    let _ = enter(&mut harness, Some(ContentPoint { line: 1, column: 2 }));
    assert_eq!(
        harness.controller.select_mode_cursor(),
        Some(ContentPoint { line: 1, column: 1 })
    );
    let _ = key(&mut harness, KeyCode::Char('V'));
    assert_eq!(
        harness.controller.selection().unwrap().unit(),
        SelectionUnit::Line
    );
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("a界tail")
    );
    let copy = key(&mut harness, KeyCode::Enter);
    assert!(matches!(copy.outcome, MouseOutcome::Copy { ref text, .. } if text == "a界tail"));
    assert!(harness.controller.select_mode_target().is_none());
    assert!(harness.controller.selection().is_none());
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 4 }));
    let _ = key(&mut harness, KeyCode::Char('W'));
    let copy = key(&mut harness, KeyCode::Char('y'));
    assert!(matches!(copy.outcome, MouseOutcome::Copy { ref text, .. } if text == "two"));
    assert!(harness.controller.select_mode_target().is_none());
}

#[test]
fn motions_cross_wrapped_wide_rows_and_clamp_at_content_limits() {
    use SelectionCommand::Move;
    use SelectionMotion::{
        Down, FirstLine, HalfPageDown, HalfPageUp, LastLine, Left, LineEnd, LineStart, PageDown,
        PageUp, Right, Up, WordBackward, WordForward,
    };
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(
        &["one two", "a界", "tail", "last row", "five", "six"],
        &mut scroll,
    ));
    harness.surfaces.surface = harness.surfaces.surface.with_wrapped_rows(&[false, true]);
    harness.areas.clear();
    harness
        .areas
        .push(ratatui::layout::Rect::new(10, 5, 3, 4), TestTarget::Left);
    let _ = enter(&mut harness, Some(ContentPoint { line: 1, column: 0 }));
    for (motion, expected) in [
        (Right, (1, 1)),
        (Right, (2, 0)),
        (Left, (1, 1)),
        (LineEnd, (2, 3)),
        (LineStart, (1, 0)),
        (Up, (0, 0)),
        (WordForward, (0, 4)),
        (WordBackward, (0, 0)),
        (HalfPageDown, (2, 0)),
        (PageDown, (5, 0)),
        (Down, (5, 0)),
        (LastLine, (5, 0)),
        (HalfPageUp, (3, 0)),
        (PageUp, (0, 0)),
        (FirstLine, (0, 0)),
        (Up, (0, 0)),
        (Left, (0, 0)),
    ] {
        let _ = command(&mut harness, Move(motion));
        assert_visible_cursor(&harness, expected, motion);
    }
}

fn assert_visible_cursor(
    harness: &ControllerHarness<TextSurfaces<'_>>,
    expected: (i64, u32),
    motion: SelectionMotion,
) {
    assert_eq!(
        harness.controller.select_mode_cursor(),
        Some(ContentPoint {
            line: expected.0,
            column: expected.1
        }),
        "{motion:?}"
    );
    let origin = harness.surfaces.origin();
    assert!((origin.line..origin.line + 4).contains(&expected.0));
    if expected.0 == 5 {
        assert_eq!(origin.line, 2);
    }
    if expected.0 == 0 {
        assert_eq!(origin.line, 0);
    }
}

#[test]
fn select_mode_intercepts_before_pane_forwarding() {
    use ratatui_mouse::{MouseEncoding, MouseTracking};
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["one two"], &mut scroll));
    harness.surfaces.request = Some(PaneMouseRequest {
        tracking: MouseTracking::ButtonMotion,
        encoding: MouseEncoding::Sgr,
        is_alternate_screen: true,
        is_application_cursor: false,
    });
    let _ = enter(&mut harness, None);
    assert_key_interception(&mut harness);
    let _ = command(&mut harness, SelectionCommand::SelectWord);
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("one")
    );
    let wheel = harness.mouse(MouseEventKind::ScrollDown, 10, 5, KeyModifiers::NONE);
    assert!(matches!(wheel.outcome, MouseOutcome::Wheel { .. }));
    let press = harness.mouse(
        MouseEventKind::Down(MouseButton::Left),
        10,
        5,
        KeyModifiers::NONE,
    );
    assert!(matches!(press.outcome, MouseOutcome::Pressed { .. }));
    assert!(harness.controller.selection().is_some());
    let _ = harness.mouse(
        MouseEventKind::Down(MouseButton::Left),
        30,
        5,
        KeyModifiers::NONE,
    );
    assert!(harness.controller.select_mode_target().is_none());
}

fn assert_key_interception(harness: &mut ControllerHarness<TextSurfaces<'_>>) {
    for should_map in [true, false] {
        harness
            .controller
            .settings_mut()
            .should_handle_select_mode_keys = should_map;
        assert_eq!(
            key(harness, KeyCode::Char('z')).outcome,
            MouseOutcome::Consumed
        );
        let previous_cursor = harness.controller.select_mode_cursor();
        assert_eq!(
            key(harness, KeyCode::Char('l')).outcome,
            MouseOutcome::Consumed
        );
        if should_map {
            assert_eq!(
                harness.controller.select_mode_cursor(),
                Some(ContentPoint { line: 0, column: 1 })
            );
        } else {
            assert_eq!(harness.controller.select_mode_cursor(), previous_cursor);
        }
        let release = harness.event(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));
        assert_eq!(release.outcome, MouseOutcome::Ignored);
        assert!(harness.controller.select_mode_target().is_some());
    }
}

#[test]
fn entry_requires_an_area_surface_and_content() {
    for missing in 0..3 {
        let mut scroll = ScrollPosition::default();
        let rows: &[&'static str] = if missing == 2 { &[] } else { &["abc"] };
        let mut harness = ControllerHarness::new(create_surfaces(rows, &mut scroll));
        if missing == 0 {
            harness.areas.clear();
        }
        if missing == 1 {
            harness.surfaces.is_present = false;
        }
        assert_eq!(enter(&mut harness, None).outcome, MouseOutcome::Ignored);
        assert!(harness.controller.select_mode_target().is_none());
    }
}

#[test]
fn target_removal_and_custom_commands_leave_no_pending_mode() {
    for is_missing_area in [false, true] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
        let _ = enter(&mut harness, None);
        if is_missing_area {
            harness.areas.clear();
        } else {
            harness.surfaces.is_present = false;
        }
        let _ = command(&mut harness, SelectionCommand::ToggleCellSelection);
        assert!(harness.controller.select_mode_target().is_none());
    }
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    let _ = enter(&mut harness, None);
    assert!(harness.controller.forget_target(&TestTarget::Left));
    assert!(harness.controller.select_mode_target().is_none());
    assert_eq!(
        command(&mut harness, SelectionCommand::Copy).outcome,
        MouseOutcome::Ignored
    );
}

#[test]
fn key_motions_report_cursor_redraw_once_through_shared_timing() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    harness.controller.settings_mut().redraw_interval = std::time::Duration::from_millis(16);
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 0 }));
    assert!(key(&mut harness, KeyCode::Right).is_redraw_due);
    assert!(!key(&mut harness, KeyCode::Left).is_redraw_due);
    assert_eq!(
        harness.controller.next_deadline(),
        Some(harness.now + std::time::Duration::from_millis(16))
    );
    harness.now += std::time::Duration::from_millis(16);
    let report = harness
        .controller
        .tick(harness.now, &harness.areas, &mut harness.surfaces);
    assert!(report.is_redraw_due);
    assert_eq!(harness.controller.next_deadline(), None);
}

#[test]
fn cell_selection_copies_wrapped_wide_endpoints_and_keeps_its_anchor() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["a界", "tail"], &mut scroll));
    harness.surfaces.surface = harness.surfaces.surface.with_wrapped_rows(&[true]);
    harness.areas.clear();
    harness
        .areas
        .push(ratatui::layout::Rect::new(10, 5, 3, 4), TestTarget::Left);
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 1 }));
    let _ = command(&mut harness, SelectionCommand::ToggleCellSelection);
    for (motion, text) in [
        (SelectionMotion::Right, "界t"),
        (SelectionMotion::Left, "界"),
        (SelectionMotion::Left, "a界"),
    ] {
        let _ = command(&mut harness, SelectionCommand::Move(motion));
        assert_eq!(
            harness
                .controller
                .selected_text(&mut harness.surfaces)
                .as_deref(),
            Some(text)
        );
    }
    let copied = command(&mut harness, SelectionCommand::Copy);
    assert!(matches!(copied.outcome, MouseOutcome::Copy { ref text, .. } if text == "a界"));
}

#[test]
fn clearing_selection_allows_a_new_keyboard_anchor() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 0 }));
    let _ = command(&mut harness, SelectionCommand::ToggleCellSelection);
    assert!(harness.controller.clear_selection());
    let _ = command(&mut harness, SelectionCommand::ToggleCellSelection);
    let _ = command(&mut harness, SelectionCommand::Move(SelectionMotion::Right));
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("ab")
    );
}

#[test]
fn blank_keyboard_copy_leaves_without_emitting_empty_clipboard_text() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["   "], &mut scroll));
    let _ = enter(&mut harness, None);
    let _ = command(&mut harness, SelectionCommand::SelectWord);
    assert_eq!(
        command(&mut harness, SelectionCommand::Copy).outcome,
        MouseOutcome::Consumed
    );
    assert!(harness.controller.select_mode_target().is_none());
    assert!(harness.controller.selection().is_none());
}

#[test]
fn wrapped_row_motion_visits_trailing_cells_before_the_next_row() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["a界  ", "tail  "], &mut scroll));
    harness.surfaces.surface = harness.surfaces.surface.with_wrapped_rows(&[true]);
    harness.areas.clear();
    harness
        .areas
        .push(ratatui::layout::Rect::new(10, 5, 5, 4), TestTarget::Left);
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 1 }));
    for expected in [(0, 3), (0, 4), (1, 0)] {
        let _ = command(&mut harness, SelectionCommand::Move(SelectionMotion::Right));
        assert_eq!(
            harness.controller.select_mode_cursor(),
            Some(ContentPoint {
                line: expected.0,
                column: expected.1
            })
        );
    }
    let _ = command(
        &mut harness,
        SelectionCommand::Move(SelectionMotion::LineEnd),
    );
    assert_eq!(
        harness.controller.select_mode_cursor(),
        Some(ContentPoint { line: 1, column: 5 })
    );
}

#[test]
fn cancelling_without_selection_cancels_the_pending_pointer_release() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["abc"], &mut scroll));
    let _ = enter(&mut harness, None);
    let _ = harness.mouse(
        MouseEventKind::Down(MouseButton::Right),
        10,
        5,
        KeyModifiers::NONE,
    );
    let _ = command(&mut harness, SelectionCommand::Cancel);
    assert!(harness.controller.select_mode_target().is_none());
    assert!(harness.controller.gesture_owner().is_none());
    let release = harness.mouse(
        MouseEventKind::Up(MouseButton::Right),
        10,
        5,
        KeyModifiers::NONE,
    );
    assert_eq!(release.outcome, MouseOutcome::Ignored);
}

#[test]
fn pointer_click_allows_keyboard_cell_selection_to_restart() {
    let mut scroll = ScrollPosition::default();
    let mut harness = ControllerHarness::new(create_surfaces(&["one two"], &mut scroll));
    let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 0 }));
    let _ = key(&mut harness, KeyCode::Char('v'));
    let _ = harness.mouse(
        MouseEventKind::Down(MouseButton::Left),
        14,
        5,
        KeyModifiers::NONE,
    );
    let _ = harness.mouse(
        MouseEventKind::Up(MouseButton::Left),
        14,
        5,
        KeyModifiers::NONE,
    );
    assert!(harness.controller.selection().is_none());
    assert_eq!(
        harness.controller.select_mode_target(),
        Some(&TestTarget::Left)
    );
    let _ = key(&mut harness, KeyCode::Char('v'));
    let _ = key(&mut harness, KeyCode::Right);
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("on")
    );
}

#[test]
fn pointer_selection_stops_previous_word_or_line_keyboard_extension() {
    for command_to_replace in [
        SelectionCommand::SelectWord,
        SelectionCommand::ToggleLineSelection,
    ] {
        let mut scroll = ScrollPosition::default();
        let mut harness = ControllerHarness::new(create_surfaces(&["one two"], &mut scroll));
        let _ = enter(&mut harness, Some(ContentPoint { line: 0, column: 0 }));
        let _ = command(&mut harness, command_to_replace);
        let _ = harness.mouse(
            MouseEventKind::Down(MouseButton::Left),
            14,
            5,
            KeyModifiers::NONE,
        );
        let _ = harness.mouse(
            MouseEventKind::Drag(MouseButton::Left),
            15,
            5,
            KeyModifiers::NONE,
        );
        assert_eq!(
            harness.controller.selection().unwrap().unit(),
            SelectionUnit::Cell
        );
        let _ = key(&mut harness, KeyCode::Right);
        assert_pointer_selection_unchanged(&mut harness);
        let _ = harness.mouse(
            MouseEventKind::Up(MouseButton::Left),
            15,
            5,
            KeyModifiers::NONE,
        );
        let _ = key(&mut harness, KeyCode::Right);
        assert_pointer_selection_unchanged(&mut harness);
    }
}

fn assert_pointer_selection_unchanged(harness: &mut ControllerHarness<TextSurfaces<'_>>) {
    assert_eq!(
        harness
            .controller
            .selected_text(&mut harness.surfaces)
            .as_deref(),
        Some("tw")
    );
}
