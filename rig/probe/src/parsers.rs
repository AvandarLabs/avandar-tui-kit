//! Pure byte-slice parsers for raw terminal capability replies and the `DA1` fence.

/// Status code reported by a terminal in a `DECRPM` (`CSI ? <mode> ; <state> $ y`) reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeReportState {
    /// Mode is not recognized by the terminal (`0`).
    NotRecognized,
    /// Mode is currently enabled (`1`).
    Set,
    /// Mode is currently disabled (`2`).
    Reset,
    /// Mode is permanently enabled (`3`).
    PermanentlySet,
    /// Mode is permanently disabled (`4`).
    PermanentlyReset,
    /// Any non-standard `DECRPM` status code outside `0..=4`.
    Unknown(u16),
}

impl ModeReportState {
    /// Maps a raw `DECRPM` status integer into a typed [`ModeReportState`].
    #[must_use]
    pub const fn from_code(code: u16) -> Self {
        match code {
            0 => Self::NotRecognized,
            1 => Self::Set,
            2 => Self::Reset,
            3 => Self::PermanentlySet,
            4 => Self::PermanentlyReset,
            other_code => Self::Unknown(other_code),
        }
    }

    /// Returns the canonical snake-case label used in probe JSONL records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotRecognized => "not_recognized",
            Self::Set => "set",
            Self::Reset => "reset",
            Self::PermanentlySet => "permanently_set",
            Self::PermanentlyReset => "permanently_reset",
            Self::Unknown(_) => "unknown",
        }
    }
}

/// Decoded `DECRPM` private mode report (`CSI ? <mode> ; <state> $ y`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeReport {
    /// DEC private mode number (for example `1002` or `2004`).
    pub mode: u16,
    /// Reported status for `mode`.
    pub state: ModeReportState,
}

/// One-based cursor coordinates reported by `CPR` (`CSI <row> ; <column> R`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    /// One-based row coordinate (`>= 1`).
    pub row: u16,
    /// One-based column coordinate (`>= 1`).
    pub column: u16,
}

/// Cell dimensions in pixels reported by `CSI 6 ; <height> ; <width> t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPixelSize {
    /// Cell height in pixels (`>= 1`).
    pub height_px: u16,
    /// Cell width in pixels (`>= 1`).
    pub width_px: u16,
}

/// Extracts all `DECRPM` replies (`CSI ? <mode> ; <state> $ y`) in arrival order.
#[must_use]
pub fn parse_mode_reports(bytes: &[u8]) -> Vec<ModeReport> {
    let mut reports = Vec::new();
    let mut scan_index = 0;
    while scan_index < bytes.len() {
        if let Some((report, next_index)) = parse_mode_report_at(bytes, scan_index) {
            reports.push(report);
            scan_index = next_index;
        } else {
            scan_index += 1;
        }
    }
    reports
}

/// Returns the most recent reported [`ModeReportState`] for `target_mode` in `reports`.
#[must_use]
pub fn latest_mode_state(reports: &[ModeReport], target_mode: u16) -> Option<ModeReportState> {
    reports
        .iter()
        .rev()
        .find(|report| report.mode == target_mode)
        .map(|report| report.state)
}

/// Extracts the terminal version string from a `DCS > | <version> ST` (`XTVERSION`) reply.
#[must_use]
pub fn parse_xtversion_reply(bytes: &[u8]) -> Option<String> {
    let mut scan_index = 0;
    while scan_index < bytes.len() {
        if !bytes[scan_index..].starts_with(b"\x1bP>|") {
            scan_index += 1;
            continue;
        }
        let body_start = scan_index + 4;
        let mut body_end = body_start;
        while body_end < bytes.len() {
            if bytes[body_end] == b'\x1b' {
                if bytes.get(body_end + 1) == Some(&b'\\') {
                    return Some(
                        String::from_utf8_lossy(&bytes[body_start..body_end]).into_owned(),
                    );
                }
                break;
            }
            body_end += 1;
        }
        scan_index = body_end;
    }
    None
}

/// Extracts all Kitty keyboard `<flags>` replies (`CSI ? <flags> u` or `CSI ? <flags> ; <mode> u`) in arrival order.
#[must_use]
pub fn parse_kitty_flags_replies(bytes: &[u8]) -> Vec<u16> {
    let mut flag_values = Vec::new();
    let mut scan_index = 0;
    while scan_index < bytes.len() {
        if let Some((flags, next_index)) = parse_kitty_flags_at(bytes, scan_index) {
            flag_values.push(flags);
            scan_index = next_index;
        } else {
            scan_index += 1;
        }
    }
    flag_values
}

/// Extracts the latest valid one-based cursor position from `CSI <row> ; <column> R` replies.
#[must_use]
pub fn parse_cursor_position(bytes: &[u8]) -> Option<CursorPosition> {
    let mut latest_position = None;
    let mut scan_index = 0;
    while scan_index < bytes.len() {
        if let Some((position, next_index)) = parse_cursor_position_at(bytes, scan_index) {
            latest_position = Some(position);
            scan_index = next_index;
        } else {
            scan_index += 1;
        }
    }
    latest_position
}

/// Extracts cell pixel dimensions from a `CSI 6 ; <height> ; <width> t` reply.
#[must_use]
pub fn parse_cell_pixel_size(bytes: &[u8]) -> Option<CellPixelSize> {
    (0..bytes.len()).find_map(|scan_index| parse_cell_pixel_size_at(bytes, scan_index))
}

/// Returns `true` when `bytes` contains a complete Primary Device Attributes (`DA1`) reply (`CSI ? <params> c`).
#[must_use]
pub fn has_da1_reply(bytes: &[u8]) -> bool {
    (0..bytes.len()).any(|scan_index| has_da1_reply_at(bytes, scan_index))
}

fn parse_ascii_u16(bytes: &[u8], start_index: usize) -> Option<(u16, usize)> {
    let mut cursor_index = start_index;
    let mut accumulated_value: u16 = 0;
    while let Some(&digit_byte) = bytes.get(cursor_index) {
        if !digit_byte.is_ascii_digit() {
            break;
        }
        let digit_value = u16::from(digit_byte - b'0');
        accumulated_value = accumulated_value
            .checked_mul(10)?
            .checked_add(digit_value)?;
        cursor_index += 1;
    }
    if cursor_index == start_index {
        return None;
    }
    Some((accumulated_value, cursor_index))
}

fn parse_mode_report_at(bytes: &[u8], start_index: usize) -> Option<(ModeReport, usize)> {
    if !bytes[start_index..].starts_with(b"\x1b[?") {
        return None;
    }
    let (mode_number, after_mode_index) = parse_ascii_u16(bytes, start_index + 3)?;
    if bytes.get(after_mode_index) != Some(&b';') {
        return None;
    }
    let (state_code, after_state_index) = parse_ascii_u16(bytes, after_mode_index + 1)?;
    if !bytes[after_state_index..].starts_with(b"$y") {
        return None;
    }
    let report = ModeReport {
        mode: mode_number,
        state: ModeReportState::from_code(state_code),
    };
    Some((report, after_state_index + 2))
}

fn parse_kitty_flags_at(bytes: &[u8], start_index: usize) -> Option<(u16, usize)> {
    if !bytes[start_index..].starts_with(b"\x1b[?") {
        return None;
    }
    let (flags_value, after_flags_index) = parse_ascii_u16(bytes, start_index + 3)?;
    match bytes.get(after_flags_index) {
        Some(&b'u') => Some((flags_value, after_flags_index + 1)),
        Some(&b';') => {
            let (_mode_value, after_mode_index) = parse_ascii_u16(bytes, after_flags_index + 1)?;
            if bytes.get(after_mode_index) == Some(&b'u') {
                Some((flags_value, after_mode_index + 1))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_cursor_position_at(bytes: &[u8], start_index: usize) -> Option<(CursorPosition, usize)> {
    if !bytes[start_index..].starts_with(b"\x1b[") {
        return None;
    }
    let (row, after_row_index) = parse_ascii_u16(bytes, start_index + 2)?;
    if row == 0 || bytes.get(after_row_index) != Some(&b';') {
        return None;
    }
    let (column, after_column_index) = parse_ascii_u16(bytes, after_row_index + 1)?;
    if column == 0 || bytes.get(after_column_index) != Some(&b'R') {
        return None;
    }
    Some((CursorPosition { row, column }, after_column_index + 1))
}

fn parse_cell_pixel_size_at(bytes: &[u8], start_index: usize) -> Option<CellPixelSize> {
    if !bytes[start_index..].starts_with(b"\x1b[6;") {
        return None;
    }
    let (height_px, after_height_index) = parse_ascii_u16(bytes, start_index + 4)?;
    if height_px == 0 || bytes.get(after_height_index) != Some(&b';') {
        return None;
    }
    let (width_px, after_width_index) = parse_ascii_u16(bytes, after_height_index + 1)?;
    if width_px == 0 || bytes.get(after_width_index) != Some(&b't') {
        return None;
    }
    Some(CellPixelSize {
        height_px,
        width_px,
    })
}

fn has_da1_reply_at(bytes: &[u8], start_index: usize) -> bool {
    if !bytes[start_index..].starts_with(b"\x1b[?") {
        return false;
    }
    let mut cursor_index = start_index + 3;
    loop {
        let group_start = cursor_index;
        while bytes.get(cursor_index).is_some_and(u8::is_ascii_digit) {
            cursor_index += 1;
        }
        if cursor_index == group_start {
            return false;
        }
        match bytes.get(cursor_index) {
            Some(&b';') => cursor_index += 1,
            Some(&b'c') => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_coalesced_mode_reports_and_missing_optional_replies_before_da1_fence() {
        // Simulates tmux: answers DECRPM for 1000/1002/1006, ignores Kitty (?u) and cell size (16t),
        // then answers DA1 (\x1b[?1;2c).
        let reply_bytes = b"\x1b[?1000;2$y\x1b[?1002;1$y\x1b[?1006;1$y\x1b[?1;2c";
        assert!(has_da1_reply(reply_bytes));
        assert_eq!(
            parse_mode_reports(reply_bytes),
            vec![
                ModeReport {
                    mode: 1000,
                    state: ModeReportState::Reset,
                },
                ModeReport {
                    mode: 1002,
                    state: ModeReportState::Set,
                },
                ModeReport {
                    mode: 1006,
                    state: ModeReportState::Set,
                },
            ]
        );
        assert_eq!(parse_kitty_flags_replies(reply_bytes), Vec::<u16>::new());
        assert_eq!(parse_cell_pixel_size(reply_bytes), None);
        assert_eq!(parse_xtversion_reply(reply_bytes), None);
        assert_eq!(parse_cursor_position(reply_bytes), None);
    }

    #[test]
    fn parses_xtversion_kitty_cursor_and_cell_size_replies() {
        let reply_bytes = concat!(
            "\x1bP>|xterm.js(6.0.0)\x1b\\",
            "\x1b[?0u\x1b[?1;1u",
            "\x1b[6;18;9t",
            "\x1b[1;1Rnoise\x1b[1;3R",
            "\x1b[?62;22c",
        )
        .as_bytes();
        assert_eq!(
            parse_xtversion_reply(reply_bytes).as_deref(),
            Some("xterm.js(6.0.0)")
        );
        assert_eq!(parse_kitty_flags_replies(reply_bytes), vec![0, 1]);
        assert_eq!(
            parse_cell_pixel_size(reply_bytes),
            Some(CellPixelSize {
                height_px: 18,
                width_px: 9,
            })
        );
        assert_eq!(
            parse_cursor_position(reply_bytes),
            Some(CursorPosition { row: 1, column: 3 })
        );
        assert!(has_da1_reply(reply_bytes));
        assert!(has_da1_reply(b"\x1b[?6c"));
        assert_eq!(
            parse_xtversion_reply(b"\x1bP>|v1-\xff\x1b\\").as_deref(),
            Some("v1-\u{fffd}")
        );
    }

    #[test]
    fn incomplete_da1_or_malformed_csi_does_not_false_trigger_fence() {
        for malformed in [
            &b"\x1b[?1002;1$y\x1b[?62;"[..],
            b"\x1b[?u",
            b"\x1b[?c",
            b"\x1b[?1;c",
            b"\x1b[?;1c",
            b"\x1b[?1;;2c",
        ] {
            assert!(!has_da1_reply(malformed));
        }
        assert!(has_da1_reply(b"leading-noise\x00\xff\x1b[?6c"));

        let bad_kitty = b"\x1b[?u\x1b[?;1u\x1b[?1;u\x1b[?999999u";
        assert_eq!(parse_kitty_flags_replies(bad_kitty), Vec::<u16>::new());
        assert_eq!(
            parse_cursor_position(b"\x1b[0;1R\x1b[1;0R\x1b[999999;1R"),
            None
        );
        assert_eq!(
            parse_cell_pixel_size(b"\x1b[6;0;9t\x1b[6;18;0t\x1b[6;999999;9t"),
            None
        );
        assert_eq!(
            parse_cell_pixel_size(b"\x1b[6;0;9t\x1b[6;16;8t"),
            Some(CellPixelSize {
                height_px: 16,
                width_px: 8
            })
        );
        let bad_modes = b"\x1b[?999999;1$y\x1b[?1000;999999$y";
        assert_eq!(parse_mode_reports(bad_modes), Vec::<ModeReport>::new());

        assert_eq!(
            parse_xtversion_reply(b"\x1bP>|foo\x1bP>|bar\x1b\\").as_deref(),
            Some("bar")
        );
        assert_eq!(parse_xtversion_reply(b"\x1bP>|unterminated\x1b"), None);
    }

    #[test]
    fn maps_all_decrpm_mode_report_state_codes_and_labels() {
        let expected_cases = [
            (0, ModeReportState::NotRecognized, "not_recognized"),
            (1, ModeReportState::Set, "set"),
            (2, ModeReportState::Reset, "reset"),
            (3, ModeReportState::PermanentlySet, "permanently_set"),
            (4, ModeReportState::PermanentlyReset, "permanently_reset"),
            (5, ModeReportState::Unknown(5), "unknown"),
        ];
        for (raw_code, expected_state, expected_label) in expected_cases {
            let decoded_state = ModeReportState::from_code(raw_code);
            assert_eq!(decoded_state, expected_state);
            assert_eq!(decoded_state.as_str(), expected_label);
        }
    }
}
