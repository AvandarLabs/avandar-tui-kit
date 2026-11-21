//! Pure JSONL record formatters for `capability_probe` writes, sections, and summaries.

use crate::color_reply::ColorReply;
use crate::environment::EnvironmentSnapshot;
use crate::json_log::{escape_json_string, push_lowercase_hex_byte};
use crate::parsers::{CellPixelSize, ModeReport, ModeReportState, latest_mode_state};

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
        push_lowercase_hex_byte(&mut hex_output, byte);
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

fn format_json_array<T, F>(items: &[T], format_item: F) -> String
where
    F: FnMut(&T) -> String,
{
    let joined = items.iter().map(format_item).collect::<Vec<_>>().join(",");
    format!("[{joined}]")
}

fn format_mode_report_array(reports: &[ModeReport]) -> String {
    format_json_array(reports, |report| {
        format!(
            "{{\"mode\":{},\"state\":\"{}\"}}",
            report.mode,
            report.state.as_str()
        )
    })
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
    let modes_array = format_json_array(entries, |entry| {
        let after_enable_json = json_optional_str(entry.after_enable.map(ModeReportState::as_str));
        let after_disable_json =
            json_optional_str(entry.after_disable.map(ModeReportState::as_str));
        format!(
            "{{\"mode\":{},\"name\":\"{}\",\"after_enable\":{},\"after_disable\":{}}}",
            entry.mode,
            escape_json_string(entry.name),
            after_enable_json,
            after_disable_json
        )
    });
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
    let samples_array = format_json_array(measurements, |sample| {
        format!(
            "{{\"name\":\"{}\",\"category\":\"{}\",\"cells\":{}}}",
            escape_json_string(sample.name),
            escape_json_string(sample.category),
            json_optional_u16(sample.cells)
        )
    });
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
mod tests;
