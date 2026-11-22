//! Pane routing priorities, protocol bytes, and emulator-request detection.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_mouse::{
    MouseEncoding, MouseSettings, MouseTracking, PaneMouseRequest, PressRoute, WheelRoute,
    build_mouse_report, build_wheel_arrow_keys, press_route, wheel_route,
};

const fn create_request(tracking: MouseTracking, encoding: MouseEncoding) -> PaneMouseRequest {
    PaneMouseRequest {
        tracking,
        encoding,
        is_alternate_screen: false,
        is_application_cursor: false,
    }
}

const fn create_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn routing_priority_selects_before_forwarding() {
    use MouseTracking::{AnyMotion, ButtonMotion, Off, Press, PressRelease};
    use PressRoute::{Forward, Select};
    use WheelRoute::{ArrowKeys, Forward as WheelForward, Report};
    let settings = MouseSettings::default();
    let mut request = create_request(ButtonMotion, MouseEncoding::Sgr);
    request.is_alternate_screen = true;
    assert_selection_overrides();
    for (tracking, alternate, press, wheel) in [
        (Off, false, Select, Report),
        (Off, true, Select, ArrowKeys),
        (Press, true, Forward, WheelForward),
        (PressRelease, false, Forward, WheelForward),
        (ButtonMotion, false, Forward, WheelForward),
        (AnyMotion, true, Forward, WheelForward),
    ] {
        request.tracking = tracking;
        request.is_alternate_screen = alternate;
        assert_eq!(
            press_route(&request, KeyModifiers::CONTROL, false, &settings),
            press
        );
        assert_eq!(
            wheel_route(&request, KeyModifiers::CONTROL, false, &settings),
            wheel
        );
    }
}

fn assert_selection_overrides() {
    let settings = MouseSettings::default();
    let mut request = create_request(MouseTracking::ButtonMotion, MouseEncoding::Sgr);
    request.is_alternate_screen = true;
    for (modifiers, select_mode) in [
        (KeyModifiers::NONE, true),
        (KeyModifiers::SHIFT, false),
        (KeyModifiers::ALT, false),
    ] {
        assert_eq!(
            press_route(&request, modifiers, select_mode, &settings),
            PressRoute::Select
        );
        assert_eq!(
            wheel_route(&request, modifiers, select_mode, &settings),
            WheelRoute::Report
        );
        request.tracking = MouseTracking::Off;
        assert_eq!(
            wheel_route(&request, modifiers, select_mode, &settings),
            WheelRoute::Report
        );
        request.tracking = MouseTracking::ButtonMotion;
    }
}

#[test]
fn routing_respects_disabled_features() {
    let mut settings = MouseSettings::default();
    let mut request = create_request(MouseTracking::ButtonMotion, MouseEncoding::Sgr);
    request.is_alternate_screen = true;
    settings.is_passthrough_enabled = false;
    assert_eq!(
        press_route(&request, KeyModifiers::NONE, false, &settings),
        PressRoute::Select
    );
    assert_eq!(
        wheel_route(&request, KeyModifiers::NONE, false, &settings),
        WheelRoute::Report
    );
    request.tracking = MouseTracking::Off;
    assert_eq!(
        wheel_route(&request, KeyModifiers::NONE, false, &settings),
        WheelRoute::Report
    );
    settings.is_passthrough_enabled = true;
    settings.is_wheel_to_arrows_enabled = false;
    assert_eq!(
        wheel_route(&request, KeyModifiers::NONE, false, &settings),
        WheelRoute::Report
    );
    request.tracking = MouseTracking::PressRelease;
    assert_eq!(
        wheel_route(&request, KeyModifiers::NONE, false, &settings),
        WheelRoute::Forward
    );
}

#[test]
fn routing_respects_custom_selection_modifiers() {
    let mut settings = MouseSettings::default();
    let request = create_request(MouseTracking::PressRelease, MouseEncoding::Sgr);
    settings.selection_modifiers = KeyModifiers::CONTROL;
    for (modifiers, press, wheel) in [
        (
            KeyModifiers::CONTROL,
            PressRoute::Select,
            WheelRoute::Report,
        ),
        (KeyModifiers::ALT, PressRoute::Forward, WheelRoute::Forward),
    ] {
        assert_eq!(press_route(&request, modifiers, false, &settings), press);
        assert_eq!(wheel_route(&request, modifiers, false, &settings), wheel);
    }
    settings.selection_modifiers = KeyModifiers::NONE;
    assert_eq!(
        press_route(&request, KeyModifiers::NONE, false, &settings),
        PressRoute::Forward
    );
}

#[test]
fn encoders_use_pane_coordinates_and_protocol_limits() {
    let request = create_request(MouseTracking::ButtonMotion, MouseEncoding::Sgr);
    let area = Rect::new(10, 5, 20, 8);
    let left_down = create_event(MouseEventKind::Down(MouseButton::Left), 10, 5);
    let left_up = create_event(MouseEventKind::Up(MouseButton::Left), 10, 5);
    assert_eq!(
        build_mouse_report(&left_down, area, &request),
        Some(b"\x1b[<0;1;1M".to_vec())
    );
    assert_eq!(
        build_mouse_report(&left_up, area, &request),
        Some(b"\x1b[<0;1;1m".to_vec())
    );
    let bottom_right = create_event(MouseEventKind::Down(MouseButton::Left), 29, 12);
    assert_eq!(
        build_mouse_report(&bottom_right, area, &request),
        Some(b"\x1b[<0;20;8M".to_vec())
    );
    for (column, row) in [(9, 5), (30, 5), (10, 4), (10, 13), (u16::MAX, u16::MAX)] {
        let event = create_event(MouseEventKind::Down(MouseButton::Left), column, row);
        assert_eq!(build_mouse_report(&event, area, &request), None);
    }
    for empty_area in [Rect::new(10, 5, 0, 8), Rect::new(10, 5, 20, 0)] {
        assert_eq!(build_mouse_report(&left_down, empty_area, &request), None);
    }
    assert_protocol_limits();
}

fn assert_protocol_limits() {
    use MouseEncoding::{Default, Sgr, Utf8};
    let area = Rect::new(0, 0, u16::MAX, u16::MAX);
    let cases = [
        (Default, 222, 222, Some(b"\x1b[M \xff\xff".as_slice())),
        (Default, 223, 0, None),
        (Default, 0, 223, None),
        (Utf8, 94, 95, Some(b"\x1b[M \x7f\xc2\x80")),
        (Utf8, 2014, 2014, Some(b"\x1b[M \xdf\xbf\xdf\xbf")),
        (Utf8, 2015, 0, None),
        (Utf8, 0, 2015, None),
        (Sgr, 2015, 223, Some(b"\x1b[<0;2016;224M")),
        (Sgr, 65534, 65534, Some(b"\x1b[<0;65535;65535M")),
        (Sgr, u16::MAX, u16::MAX, None),
    ];
    for (encoding, column, row, expected) in cases {
        let request = create_request(MouseTracking::ButtonMotion, encoding);
        let event = create_event(MouseEventKind::Down(MouseButton::Left), column, row);
        assert_eq!(
            build_mouse_report(&event, area, &request).as_deref(),
            expected,
            "{encoding:?} at {column},{row}"
        );
    }
    let event = create_event(MouseEventKind::Down(MouseButton::Left), 65534, 65534);
    let request = create_request(MouseTracking::ButtonMotion, Sgr);
    assert_eq!(
        build_mouse_report(&event, Rect::new(65530, 65530, 20, 20), &request),
        Some(b"\x1b[<0;5;5M".to_vec())
    );
}

struct ButtonCase {
    kind: MouseEventKind,
    codes: [u8; 8],
    is_release: bool,
}

fn create_button_cases() -> Vec<ButtonCase> {
    use MouseButton::{Left, Middle, Right};
    use MouseEventKind::{Down, Drag, ScrollDown, ScrollLeft, ScrollRight, ScrollUp, Up};
    [
        (Down(Left), [0, 4, 8, 16, 12, 20, 24, 28], false),
        (Down(Middle), [1, 5, 9, 17, 13, 21, 25, 29], false),
        (Down(Right), [2, 6, 10, 18, 14, 22, 26, 30], false),
        (Up(Left), [0, 4, 8, 16, 12, 20, 24, 28], true),
        (Up(Middle), [1, 5, 9, 17, 13, 21, 25, 29], true),
        (Up(Right), [2, 6, 10, 18, 14, 22, 26, 30], true),
        (Drag(Left), [32, 36, 40, 48, 44, 52, 56, 60], false),
        (Drag(Middle), [33, 37, 41, 49, 45, 53, 57, 61], false),
        (Drag(Right), [34, 38, 42, 50, 46, 54, 58, 62], false),
        (ScrollUp, [64, 68, 72, 80, 76, 84, 88, 92], false),
        (ScrollDown, [65, 69, 73, 81, 77, 85, 89, 93], false),
        (ScrollLeft, [66, 70, 74, 82, 78, 86, 90, 94], false),
        (ScrollRight, [67, 71, 75, 83, 79, 87, 91, 95], false),
    ]
    .into_iter()
    .map(|(kind, codes, is_release)| ButtonCase {
        kind,
        codes,
        is_release,
    })
    .collect()
}

#[test]
fn encoders_preserve_buttons_and_all_modifier_combinations() {
    use KeyModifiers as Modifiers;
    let modifiers = [
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CONTROL,
        Modifiers::SHIFT | Modifiers::ALT,
        Modifiers::SHIFT | Modifiers::CONTROL,
        Modifiers::ALT | Modifiers::CONTROL,
        Modifiers::SHIFT | Modifiers::ALT | Modifiers::CONTROL,
    ];
    for case in create_button_cases() {
        for (index, &held_modifiers) in modifiers.iter().enumerate() {
            let mut event = create_event(case.kind, 10, 5);
            event.modifiers = held_modifiers;
            let release_codes = [3, 7, 11, 19, 15, 23, 27, 31];
            assert_button_encodings(
                event,
                case.codes[index],
                release_codes[index],
                case.is_release,
            );
        }
    }
}

fn assert_button_encodings(event: MouseEvent, code: u8, release_code: u8, is_release: bool) {
    for encoding in [
        MouseEncoding::Default,
        MouseEncoding::Utf8,
        MouseEncoding::Sgr,
    ] {
        let request = create_request(MouseTracking::ButtonMotion, encoding);
        let expected = if encoding == MouseEncoding::Sgr {
            let suffix = if is_release { 'm' } else { 'M' };
            format!("\x1b[<{code};1;1{suffix}").into_bytes()
        } else {
            let legacy_code = if is_release { release_code } else { code };
            vec![0x1b, b'[', b'M', legacy_code + 32, 33, 33]
        };
        assert_eq!(
            build_mouse_report(&event, Rect::new(10, 5, 20, 8), &request),
            Some(expected),
            "{event:?} {encoding:?}"
        );
    }
}

#[test]
fn tracking_filters_and_cursor_arrows() {
    let kinds = [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
        MouseEventKind::ScrollLeft,
        MouseEventKind::ScrollRight,
        MouseEventKind::Moved,
    ];
    for (tracking, expected) in create_tracking_cases() {
        for encoding in [
            MouseEncoding::Default,
            MouseEncoding::Utf8,
            MouseEncoding::Sgr,
        ] {
            let request = create_request(tracking, encoding);
            for (kind, should_forward) in kinds.into_iter().zip(expected) {
                let event = create_event(kind, 10, 5);
                assert_eq!(
                    build_mouse_report(&event, Rect::new(10, 5, 20, 8), &request).is_some(),
                    should_forward,
                    "{tracking:?} {encoding:?} {kind:?}"
                );
            }
        }
    }
}

const fn create_tracking_cases() -> [(MouseTracking, [bool; 8]); 5] {
    use MouseTracking::{AnyMotion, ButtonMotion, Off, Press, PressRelease};
    [
        (
            Off,
            [false, false, false, false, false, false, false, false],
        ),
        (
            Press,
            [true, false, false, false, false, false, false, false],
        ),
        (
            PressRelease,
            [true, true, false, true, true, true, true, false],
        ),
        (
            ButtonMotion,
            [true, true, true, true, true, true, true, false],
        ),
        (AnyMotion, [true, true, true, true, true, true, true, false]),
    ]
}

#[test]
fn wheel_rows_emit_arrows_in_the_requested_cursor_mode() {
    for (rows, application, expected) in [
        (-3, false, b"\x1b[A\x1b[A\x1b[A".as_slice()),
        (3, false, b"\x1b[B\x1b[B\x1b[B"),
        (-3, true, b"\x1bOA\x1bOA\x1bOA"),
        (3, true, b"\x1bOB\x1bOB\x1bOB"),
        (0, false, b""),
        (0, true, b""),
    ] {
        assert_eq!(build_wheel_arrow_keys(rows, application), expected);
    }
}

#[cfg(feature = "vt100")]
#[test]
fn pane_requests_map_real_parser_modes_and_encodings() {
    use ratatui_mouse::{SelectableSurface, Vt100Surface};
    for (mode, tracking) in [
        (b"".as_slice(), MouseTracking::Off),
        (b"\x1b[?9h", MouseTracking::Press),
        (b"\x1b[?1000h", MouseTracking::PressRelease),
        (b"\x1b[?1002h", MouseTracking::ButtonMotion),
        (b"\x1b[?1003h", MouseTracking::AnyMotion),
    ] {
        for (sequence, encoding) in [
            (b"".as_slice(), MouseEncoding::Default),
            (b"\x1b[?1005h", MouseEncoding::Utf8),
            (b"\x1b[?1006h", MouseEncoding::Sgr),
        ] {
            for (flags, alternate, application) in [
                (b"".as_slice(), false, false),
                (b"\x1b[?1049h", true, false),
                (b"\x1b[?1h", false, true),
                (b"\x1b[?1049h\x1b[?1h", true, true),
            ] {
                let mut parser = vt100::Parser::new(3, 8, 0);
                parser.process(mode);
                parser.process(sequence);
                parser.process(flags);
                let expected = PaneMouseRequest {
                    tracking,
                    encoding,
                    is_alternate_screen: alternate,
                    is_application_cursor: application,
                };
                assert_eq!(PaneMouseRequest::from_screen(parser.screen()), expected);
                let surface = Vt100Surface::new(parser.screen_mut());
                assert_eq!(surface.pane_request(), Some(expected));
            }
        }
    }
}

#[cfg(feature = "vt100")]
#[test]
fn pane_requests_reflect_mode_resets() {
    let mut parser = vt100::Parser::new(3, 8, 0);
    parser.process(b"\x1b[?1003h\x1b[?1006h\x1b[?1049h\x1b[?1h");
    assert_eq!(
        PaneMouseRequest::from_screen(parser.screen()),
        PaneMouseRequest {
            tracking: MouseTracking::AnyMotion,
            encoding: MouseEncoding::Sgr,
            is_alternate_screen: true,
            is_application_cursor: true,
        }
    );
    parser.process(b"\x1b[?1003l\x1b[?1006l\x1b[?1049l\x1b[?1l");
    assert_eq!(
        PaneMouseRequest::from_screen(parser.screen()),
        create_request(MouseTracking::Off, MouseEncoding::Default)
    );
}
