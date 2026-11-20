//! Pure JSONL record formatters for `capability_probe` writes, sections, and summaries.

use std::fmt::Write as _;

use crate::color_reply::ColorReply;
use crate::environment::EnvironmentSnapshot;
use crate::json_log::escape_json_string;
use crate::parsers::{CellPixelSize, ModeReport, ModeReportState, latest_mode_state};

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Outcome of enabling and disabling a single DEC private mode with `DECRPM` verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingleModeProbe {
    /// DEC private mode number (`2004`, `1004`, or `2026`).
    pub mode: u16,
    /// Human-readable snake-case mode identifier.
    pub name: &'static str,
    /// Reported `DECRPM` state immediately after sending the mode-enable sequence.
    pub after_enable: Option<ModeReportState>,
    /// Reported `DECRPM` state immediately after sending the mode-disable sequence.
    pub after_disable: Option<ModeReportState>,
}

/// Width measurement for one [`crate::widths::WidthSample`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasuredWidthSample {
    /// Sample identifier from [`crate::widths::WIDTH_SAMPLES`].
    pub name: &'static str,
    /// Sample category from [`crate::widths::WIDTH_SAMPLES`].
    pub category: &'static str,
    /// Measured rendered cell count, or `None` if cursor reporting was unavailable.
    pub cells: Option<u16>,
}

fn encode_lowercase_hex(bytes: &[u8]) -> String {
    let mut hex_output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        hex_output.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        hex_output.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    hex_output
}

fn json_optional_str(value: Option<&str>) -> String {
    value.map_or_else(
        || "null".to_owned(),
        |text| format!("\"{}\"", escape_json_string(text)),
    )
}

fn json_optional_u16(value: Option<u16>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| number.to_string())
}

fn format_mode_report_array(reports: &[ModeReport]) -> String {
    let mut output = String::from("[");
    for (index, report) in reports.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        let _ = write!(
            output,
            "{{\"mode\":{},\"state\":\"{}\"}}",
            report.mode,
            report.state.as_str()
        );
    }
    output.push(']');
    output
}

/// Formats a `{"kind":"write",...}` JSONL record for raw bytes written to the terminal.
#[must_use]
pub fn format_write_record(label: &str, bytes: &[u8]) -> String {
    format!(
        "{{\"kind\":\"write\",\"label\":\"{}\",\"hex\":\"{}\"}}",
        escape_json_string(label),
        encode_lowercase_hex(bytes)
    )
}

/// Formats the `identity` section JSONL record combining `XTVERSION` and environment variables.
#[must_use]
pub fn format_identity_section(
    snapshot: &EnvironmentSnapshot,
    is_tty_available: bool,
    xtversion: Option<&str>,
    has_timed_out: bool,
) -> String {
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"identity\",",
            "\"is_tty_available\":{},\"timed_out\":{},\"xtversion\":{},",
            "\"os\":\"{}\",\"arch\":\"{}\",\"term\":{},\"term_program\":{},",
            "\"term_program_version\":{},\"is_tmux\":{},\"is_ssh\":{},",
            "\"has_no_color\":{},\"orca_version\":{},\"is_stdin_tty\":{},\"is_stdout_tty\":{}}}"
        ),
        is_tty_available,
        has_timed_out,
        json_optional_str(xtversion),
        escape_json_string(snapshot.os),
        escape_json_string(snapshot.arch),
        json_optional_str(snapshot.term.as_deref()),
        json_optional_str(snapshot.term_program.as_deref()),
        json_optional_str(snapshot.term_program_version.as_deref()),
        snapshot.is_tmux,
        snapshot.is_ssh,
        snapshot.has_no_color,
        json_optional_str(snapshot.orca_version.as_deref()),
        snapshot.is_stdin_tty,
        snapshot.is_stdout_tty
    )
}

/// Formats the `mouse_modes` section JSONL record for rules M1 and M5 `DECRPM` checks.
#[must_use]
pub fn format_mouse_modes_section(
    initial_reports: &[ModeReport],
    after_m1_reports: &[ModeReport],
    after_m5_reports: &[ModeReport],
    has_timed_out: bool,
) -> String {
    let is_m1_expected = latest_mode_state(after_m1_reports, 1000) == Some(ModeReportState::Reset)
        && latest_mode_state(after_m1_reports, 1002) == Some(ModeReportState::Set)
        && latest_mode_state(after_m1_reports, 1003) == Some(ModeReportState::Reset)
        && latest_mode_state(after_m1_reports, 1006) == Some(ModeReportState::Set);
    let is_m5_reset = [1000_u16, 1002, 1003, 1006]
        .into_iter()
        .all(|mode| latest_mode_state(after_m5_reports, mode) == Some(ModeReportState::Reset));
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"mouse_modes\",\"timed_out\":{},",
            "\"is_m1_expected\":{},\"is_m5_reset\":{},",
            "\"initial\":{},\"after_m1\":{},\"after_m5\":{}}}"
        ),
        has_timed_out,
        is_m1_expected,
        is_m5_reset,
        format_mode_report_array(initial_reports),
        format_mode_report_array(after_m1_reports),
        format_mode_report_array(after_m5_reports)
    )
}

/// Formats the `kitty_keyboard` section JSONL record for query, push (`>1u`), and pop (`<u`).
#[must_use]
pub fn format_kitty_section(
    initial_flags: Option<u16>,
    pushed_flags: Option<u16>,
    popped_flags: Option<u16>,
    has_timed_out: bool,
) -> String {
    let is_answered = initial_flags.is_some() || pushed_flags.is_some() || popped_flags.is_some();
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"kitty_keyboard\",\"timed_out\":{},",
            "\"answered\":{},\"initial_flags\":{},\"pushed_flags\":{},\"popped_flags\":{}}}"
        ),
        has_timed_out,
        is_answered,
        json_optional_u16(initial_flags),
        json_optional_u16(pushed_flags),
        json_optional_u16(popped_flags)
    )
}

/// Formats the `modes` section JSONL record for bracketed paste, focus, and synchronized output.
#[must_use]
pub fn format_modes_section(entries: &[SingleModeProbe], has_timed_out: bool) -> String {
    let mut modes_array = String::from("[");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            modes_array.push(',');
        }
        let enable_str = json_optional_str(entry.after_enable.map(ModeReportState::as_str));
        let disable_str = json_optional_str(entry.after_disable.map(ModeReportState::as_str));
        let _ = write!(
            modes_array,
            "{{\"mode\":{},\"name\":\"{}\",\"after_enable\":{},\"after_disable\":{}}}",
            entry.mode,
            escape_json_string(entry.name),
            enable_str,
            disable_str
        );
    }
    modes_array.push(']');
    format!(
        "{{\"kind\":\"section\",\"section\":\"modes\",\"timed_out\":{has_timed_out},\"modes\":{modes_array}}}"
    )
}

/// Formats the `cell_size` section JSONL record for `CSI 16 t` pixel dimensions.
#[must_use]
pub fn format_cell_size_section(cell_size: Option<CellPixelSize>, has_timed_out: bool) -> String {
    let is_answered = cell_size.is_some();
    let height_json = json_optional_u16(cell_size.map(|dimensions| dimensions.height_px));
    let width_json = json_optional_u16(cell_size.map(|dimensions| dimensions.width_px));
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"cell_size\",\"timed_out\":{},",
            "\"answered\":{},\"height_px\":{},\"width_px\":{}}}"
        ),
        has_timed_out, is_answered, height_json, width_json
    )
}

/// Formats the `widths` section JSONL record containing all measured width samples.
#[must_use]
pub fn format_widths_section(measurements: &[MeasuredWidthSample], has_timed_out: bool) -> String {
    let mut samples_array = String::from("[");
    for (index, sample) in measurements.iter().enumerate() {
        if index > 0 {
            samples_array.push(',');
        }
        let _ = write!(
            samples_array,
            "{{\"name\":\"{}\",\"category\":\"{}\",\"cells\":{}}}",
            escape_json_string(sample.name),
            escape_json_string(sample.category),
            json_optional_u16(sample.cells)
        );
    }
    samples_array.push(']');
    format!(
        "{{\"kind\":\"section\",\"section\":\"widths\",\"timed_out\":{has_timed_out},\"samples\":{samples_array}}}"
    )
}

/// Formats the `osc52` section JSONL record describing which clipboard sequences were emitted.
#[must_use]
pub fn format_osc52_section(is_emitted: bool, is_plain_sent: bool, is_tmux_sent: bool) -> String {
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"osc52\",",
            "\"emitted\":{},\"plain_sent\":{},\"tmux_sent\":{}}}"
        ),
        is_emitted, is_plain_sent, is_tmux_sent
    )
}

/// Formats a `background_color` or `background_color_late` section JSONL record.
#[must_use]
pub fn format_background_color_section(
    section_name: &str,
    phase: &str,
    elapsed_ms: u64,
    reply: Option<&ColorReply>,
) -> String {
    let escaped_section = escape_json_string(section_name);
    let escaped_phase = escape_json_string(phase);
    let Some(color_reply) = reply else {
        return format!(
            concat!(
                "{{\"kind\":\"section\",\"section\":\"{}\",\"answered\":false,",
                "\"phase\":\"{}\",\"elapsed_ms\":{},\"r\":null,\"g\":null,\"b\":null,",
                "\"channels\":null,\"luminance\":null,\"is_dark\":null}}"
            ),
            escaped_section, escaped_phase, elapsed_ms
        );
    };
    format!(
        concat!(
            "{{\"kind\":\"section\",\"section\":\"{}\",\"answered\":true,",
            "\"phase\":\"{}\",\"elapsed_ms\":{},\"r\":{},\"g\":{},\"b\":{},",
            "\"channels\":[\"{}\",\"{}\",\"{}\"],\"luminance\":{:.6},\"is_dark\":{}}}"
        ),
        escaped_section,
        escaped_phase,
        elapsed_ms,
        color_reply.r,
        color_reply.g,
        color_reply.b,
        escape_json_string(&color_reply.channels[0]),
        escape_json_string(&color_reply.channels[1]),
        escape_json_string(&color_reply.channels[2]),
        color_reply.luminance,
        color_reply.is_dark
    )
}

/// Formats the final `{"kind":"summary",...}` JSONL record before `.ready` creation.
#[must_use]
pub fn format_summary_record(mode: &str, exit_chord: &str, panic_key: &str) -> String {
    format!(
        "{{\"kind\":\"summary\",\"mode\":\"{}\",\"exit_chord\":\"{}\",\"panic_key\":\"{}\"}}",
        escape_json_string(mode),
        escape_json_string(exit_chord),
        escape_json_string(panic_key)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_write_and_identity_section_records() {
        let write_line = format_write_record("identity:xtversion", b"\x1b[>0q\x1b[c");
        assert_eq!(
            write_line,
            r#"{"kind":"write","label":"identity:xtversion","hex":"1b5b3e30711b5b63"}"#
        );
        let snapshot = EnvironmentSnapshot {
            os: "macos",
            arch: "aarch64",
            term: Some("xterm-256color".to_owned()),
            term_program: Some("Orca".to_owned()),
            term_program_version: Some("1.4.0".to_owned()),
            is_tmux: false,
            is_ssh: false,
            has_no_color: true,
            orca_version: Some("1.4.0".to_owned()),
            is_stdin_tty: true,
            is_stdout_tty: true,
        };
        let identity_line =
            format_identity_section(&snapshot, true, Some("xterm.js(6.0.0)"), false);
        assert!(identity_line.contains(r#""section":"identity""#));
        assert!(identity_line.contains(r#""xtversion":"xterm.js(6.0.0)""#));
    }

    #[test]
    fn formats_mouse_modes_and_kitty_section_records() {
        let after_m1 = [
            ModeReport {
                mode: 1000,
                state: ModeReportState::Reset,
            },
            ModeReport {
                mode: 1002,
                state: ModeReportState::Set,
            },
            ModeReport {
                mode: 1003,
                state: ModeReportState::Reset,
            },
            ModeReport {
                mode: 1006,
                state: ModeReportState::Set,
            },
        ];
        let after_m5 = [1000, 1002, 1003, 1006].map(|mode| ModeReport {
            mode,
            state: ModeReportState::Reset,
        });
        let mouse_line = format_mouse_modes_section(&[], &after_m1, &after_m5, false);
        assert!(mouse_line.contains(r#""is_m1_expected":true,"is_m5_reset":true"#));

        let kitty_line = format_kitty_section(Some(0), Some(1), Some(0), false);
        assert!(kitty_line.contains(r#""answered":true,"initial_flags":0,"pushed_flags":1"#));
    }

    #[test]
    fn formats_modes_cell_size_widths_and_osc52_records() {
        let mode_line = format_modes_section(
            &[SingleModeProbe {
                mode: 2004,
                name: "bracketed_paste",
                after_enable: Some(ModeReportState::Set),
                after_disable: Some(ModeReportState::Reset),
            }],
            false,
        );
        assert!(mode_line.contains(r#""name":"bracketed_paste","after_enable":"set""#));

        let cell_line = format_cell_size_section(
            Some(CellPixelSize {
                height_px: 18,
                width_px: 9,
            }),
            false,
        );
        assert!(cell_line.contains(r#""answered":true,"height_px":18,"width_px":9"#));
        assert!(format_cell_size_section(None, false).contains(r#""answered":false"#));

        let widths_line = format_widths_section(
            &[MeasuredWidthSample {
                name: "ascii_a",
                category: "ascii_and_accents",
                cells: Some(1),
            }],
            false,
        );
        assert!(
            widths_line.contains(r#"{"name":"ascii_a","category":"ascii_and_accents","cells":1}"#)
        );

        let osc52_line = format_osc52_section(true, true, false);
        assert!(osc52_line.contains(r#""emitted":true,"plain_sent":true,"tmux_sent":false"#));
    }

    #[test]
    fn formats_background_color_and_summary_records() {
        let color_reply = ColorReply {
            r: 26,
            g: 27,
            b: 38,
            channels: ["1a1a".to_owned(), "1b1b".to_owned(), "2626".to_owned()],
            luminance: 0.012_345,
            is_dark: true,
        };
        let color_line =
            format_background_color_section("background_color", "initial", 14, Some(&color_reply));
        assert!(color_line.contains(r#""answered":true,"phase":"initial","elapsed_ms":14"#));
        assert!(color_line.contains(r#""is_dark":true"#));

        let timeout_line =
            format_background_color_section("background_color", "timeout", 2_000, None);
        assert!(timeout_line.contains(r#""answered":false,"phase":"timeout","elapsed_ms":2000"#));

        let summary_line = format_summary_record("batch", "Ctrl+C", "F12");
        assert_eq!(
            summary_line,
            r#"{"kind":"summary","mode":"batch","exit_chord":"Ctrl+C","panic_key":"F12"}"#
        );
    }
}
