//! Pure encoders for a pane's mouse and cursor-key protocols.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{MouseEncoding, MouseTracking, PaneMouseRequest};

/// Encode an event using one-based coordinates relative to the pane's `area`.
///
/// Return `None` for out-of-area coordinates, unsupported events, or coordinates
/// above the encoding's limit: 223 for default and 2015 for UTF-8.
#[must_use]
pub fn build_mouse_report(
    event: &MouseEvent,
    area: Rect,
    request: &PaneMouseRequest,
) -> Option<Vec<u8>> {
    if !area.contains(Position::new(event.column, event.row))
        || !tracking_allows(event.kind, request.tracking)
    {
        return None;
    }
    let column = u32::from(event.column - area.x) + 1;
    let row = u32::from(event.row - area.y) + 1;
    let code = build_button_code(*event, request.encoding)?;
    match request.encoding {
        MouseEncoding::Sgr => {
            let suffix = if matches!(event.kind, MouseEventKind::Up(_)) {
                'm'
            } else {
                'M'
            };
            Some(format!("\x1b[<{code};{column};{row}{suffix}").into_bytes())
        }
        MouseEncoding::Default => build_default_report(code, column, row),
        MouseEncoding::Utf8 => build_utf8_report(code, column, row),
    }
}

const fn tracking_allows(kind: MouseEventKind, tracking: MouseTracking) -> bool {
    match kind {
        MouseEventKind::Down(_) => !matches!(tracking, MouseTracking::Off),
        MouseEventKind::Drag(_) => matches!(
            tracking,
            MouseTracking::ButtonMotion | MouseTracking::AnyMotion
        ),
        MouseEventKind::Up(_)
        | MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => matches!(
            tracking,
            MouseTracking::PressRelease | MouseTracking::ButtonMotion | MouseTracking::AnyMotion
        ),
        MouseEventKind::Moved => false,
    }
}

fn build_button_code(event: MouseEvent, encoding: MouseEncoding) -> Option<u8> {
    let mut code = match event.kind {
        MouseEventKind::Down(button) | MouseEventKind::Drag(button) => button_to_code(button),
        MouseEventKind::Up(button) => {
            if encoding == MouseEncoding::Sgr {
                button_to_code(button)
            } else {
                3
            }
        }
        MouseEventKind::ScrollUp => 64,
        MouseEventKind::ScrollDown => 65,
        MouseEventKind::ScrollLeft => 66,
        MouseEventKind::ScrollRight => 67,
        MouseEventKind::Moved => return None,
    };
    for (modifier, value) in [
        (KeyModifiers::SHIFT, 4),
        (KeyModifiers::ALT, 8),
        (KeyModifiers::CONTROL, 16),
    ] {
        if event.modifiers.contains(modifier) {
            code += value;
        }
    }
    if matches!(event.kind, MouseEventKind::Drag(_)) {
        code += 32;
    }
    Some(code)
}

const fn button_to_code(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

fn build_default_report(code: u8, column: u32, row: u32) -> Option<Vec<u8>> {
    if column > 223 || row > 223 {
        return None;
    }
    Some(vec![
        0x1b,
        b'[',
        b'M',
        code + 32,
        u8::try_from(column + 32).ok()?,
        u8::try_from(row + 32).ok()?,
    ])
}

fn build_utf8_report(code: u8, column: u32, row: u32) -> Option<Vec<u8>> {
    if column > 2015 || row > 2015 {
        return None;
    }
    let mut report = b"\x1b[M".to_vec();
    for value in [u32::from(code) + 32, column + 32, row + 32] {
        let mut bytes = [0; 4];
        report.extend_from_slice(char::from_u32(value)?.encode_utf8(&mut bytes).as_bytes());
    }
    Some(report)
}

/// Encode negative wheel rows as up keys and positive rows as down keys.
#[must_use]
pub fn build_wheel_arrow_keys(rows: i32, is_application_cursor: bool) -> Vec<u8> {
    let prefix = if is_application_cursor { b'O' } else { b'[' };
    let direction = if rows < 0 { b'A' } else { b'B' };
    let mut report = Vec::new();
    for _ in 0..rows.unsigned_abs() {
        report.extend_from_slice(&[0x1b, prefix, direction]);
    }
    report
}
