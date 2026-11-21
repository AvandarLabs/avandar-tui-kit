//! Pure JSONL record formatters for interactive `crossterm::event::Event` records.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::json_log::escape_json_string;

fn format_key_code_label(code: KeyCode) -> String {
    match code {
        KeyCode::Backspace => "backspace".to_owned(),
        KeyCode::Enter => "enter".to_owned(),
        KeyCode::Left => "left".to_owned(),
        KeyCode::Right => "right".to_owned(),
        KeyCode::Up => "up".to_owned(),
        KeyCode::Down => "down".to_owned(),
        KeyCode::Home => "home".to_owned(),
        KeyCode::End => "end".to_owned(),
        KeyCode::PageUp => "page_up".to_owned(),
        KeyCode::PageDown => "page_down".to_owned(),
        KeyCode::Tab => "tab".to_owned(),
        KeyCode::BackTab => "back_tab".to_owned(),
        KeyCode::Delete => "delete".to_owned(),
        KeyCode::Insert => "insert".to_owned(),
        KeyCode::F(number) => format!("f{number}"),
        KeyCode::Char(character) => format!("char:{character}"),
        KeyCode::Null => "null".to_owned(),
        KeyCode::Esc => "esc".to_owned(),
        other_code => format!("other:{other_code:?}"),
    }
}

const fn mouse_button_label(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

fn format_mouse_kind_label(kind: MouseEventKind) -> String {
    match kind {
        MouseEventKind::Down(button) => format!("down:{}", mouse_button_label(button)),
        MouseEventKind::Up(button) => format!("up:{}", mouse_button_label(button)),
        MouseEventKind::Drag(button) => format!("drag:{}", mouse_button_label(button)),
        MouseEventKind::Moved => "moved".to_owned(),
        MouseEventKind::ScrollDown => "scroll_down".to_owned(),
        MouseEventKind::ScrollUp => "scroll_up".to_owned(),
        MouseEventKind::ScrollLeft => "scroll_left".to_owned(),
        MouseEventKind::ScrollRight => "scroll_right".to_owned(),
    }
}

fn format_key_event_record(key_event: KeyEvent, burst_press_count: usize) -> String {
    let kind_label = match key_event.kind {
        KeyEventKind::Press => "press",
        KeyEventKind::Repeat => "repeat",
        KeyEventKind::Release => "release",
    };
    let modifiers = key_event.modifiers;
    format!(
        concat!(
            "{{\"kind\":\"event\",\"burst_press_count\":{},\"event_type\":\"key\",",
            "\"code\":\"{}\",\"key_kind\":\"{}\",\"shift\":{},\"ctrl\":{},\"alt\":{},",
            "\"super\":{},\"hyper\":{},\"meta\":{}}}"
        ),
        burst_press_count,
        escape_json_string(&format_key_code_label(key_event.code)),
        kind_label,
        modifiers.contains(KeyModifiers::SHIFT),
        modifiers.contains(KeyModifiers::CONTROL),
        modifiers.contains(KeyModifiers::ALT),
        modifiers.contains(KeyModifiers::SUPER),
        modifiers.contains(KeyModifiers::HYPER),
        modifiers.contains(KeyModifiers::META)
    )
}

fn format_mouse_event_record(mouse_event: MouseEvent, burst_press_count: usize) -> String {
    let modifiers = mouse_event.modifiers;
    format!(
        concat!(
            "{{\"kind\":\"event\",\"burst_press_count\":{},\"event_type\":\"mouse\",",
            "\"mouse_kind\":\"{}\",\"column\":{},\"row\":{},",
            "\"shift\":{},\"ctrl\":{},\"alt\":{}}}"
        ),
        burst_press_count,
        escape_json_string(&format_mouse_kind_label(mouse_event.kind)),
        mouse_event.column,
        mouse_event.row,
        modifiers.contains(KeyModifiers::SHIFT),
        modifiers.contains(KeyModifiers::CONTROL),
        modifiers.contains(KeyModifiers::ALT)
    )
}

/// Formats an interactive `crossterm` [`Event`] JSONL record with its burst press count.
#[must_use]
pub fn format_event_record(event: &Event, burst_press_count: usize) -> String {
    match event {
        Event::Key(key_event) => format_key_event_record(*key_event, burst_press_count),
        Event::Mouse(mouse_event) => format_mouse_event_record(*mouse_event, burst_press_count),
        Event::Paste(pasted_text) => format!(
            "{{\"kind\":\"event\",\"burst_press_count\":{},\"event_type\":\"paste\",\"text\":\"{}\",\"byte_len\":{}}}",
            burst_press_count,
            escape_json_string(pasted_text),
            pasted_text.len()
        ),
        Event::FocusGained => format!(
            "{{\"kind\":\"event\",\"burst_press_count\":{burst_press_count},\"event_type\":\"focus\",\"focused\":true}}"
        ),
        Event::FocusLost => format!(
            "{{\"kind\":\"event\",\"burst_press_count\":{burst_press_count},\"event_type\":\"focus\",\"focused\":false}}"
        ),
        Event::Resize(columns, rows) => format!(
            "{{\"kind\":\"event\",\"burst_press_count\":{burst_press_count},\"event_type\":\"resize\",\"columns\":{columns},\"rows\":{rows}}}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_event_record_serializes_press_code_and_active_modifier_flags() {
        let key_event = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            KeyEventKind::Press,
        ));
        let key_line = format_event_record(&key_event, 1);
        assert!(key_line.contains(r#""event_type":"key","code":"enter","key_kind":"press""#));
        assert!(key_line.contains(r#""shift":true,"ctrl":false,"alt":true"#));
    }

    #[test]
    fn mouse_event_record_serializes_button_action_coordinates_and_modifiers() {
        let mouse_event = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 4,
            modifiers: KeyModifiers::SHIFT,
        });
        let mouse_line = format_event_record(&mouse_event, 0);
        assert!(mouse_line.contains(r#""mouse_kind":"down:left","column":12,"row":4"#));
    }

    #[test]
    fn paste_focus_and_resize_events_serialize_escaped_payloads_and_dimensions() {
        let paste_line = format_event_record(&Event::Paste("a\nb".to_owned()), 0);
        assert!(paste_line.contains(r#""event_type":"paste","text":"a\nb","byte_len":3"#));
        assert!(format_event_record(&Event::FocusLost, 0).contains(r#""focused":false"#));
        assert!(format_event_record(&Event::Resize(120, 40), 0).contains(r#""columns":120"#));
    }
}
