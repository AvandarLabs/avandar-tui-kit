//! Deterministic capability probe section execution (`identity` through `background_color`).

use std::io;

use crate::armed_color::{ArmedColorListener, run_background_color_section};
use crate::environment::{EnvironmentSnapshot, read_process_environment};
use crate::json_log::JsonLog;
use crate::parsers::{
    ModeReport, ModeReportState, parse_cell_pixel_size, parse_kitty_flags_replies,
    parse_mode_reports, parse_xtversion_reply,
};
use crate::probe_args::ProbeArgs;
use crate::section_json::{
    MeasuredWidthSample, SingleModeProbe, format_cell_size_section, format_identity_section,
    format_kitty_section, format_modes_section, format_mouse_modes_section, format_osc52_section,
    format_widths_section,
};
use crate::sequences::{
    BRACKETED_PASTE_OFF, BRACKETED_PASTE_ON, CELL_SIZE_PIXELS_QUERY, FOCUS_EVENTS_OFF,
    FOCUS_EVENTS_ON, KITTY_KEYBOARD_POP, KITTY_KEYBOARD_PUSH_DISAMBIGUATE, KITTY_KEYBOARD_QUERY,
    MOUSE_SETUP_M1, MOUSE_TEARDOWN_M5, SYNC_OUTPUT_OFF, SYNC_OUTPUT_ON, XTVERSION_QUERY,
    build_decrpm_query, build_osc52_plain, build_osc52_tmux,
};
use crate::terminal_io::{RawQuerySession, TerminalSessionGuard, TerminalWriter};
use crate::widths::WIDTH_SAMPLES;

fn build_mouse_decrpm_queries() -> Vec<u8> {
    let mut payload = Vec::with_capacity(36);
    for mode in [1000_u16, 1002, 1003, 1006] {
        payload.extend_from_slice(&build_decrpm_query(mode));
    }
    payload
}

fn find_report_state(reports: &[ModeReport], target_mode: u16) -> Option<ModeReportState> {
    reports
        .iter()
        .rev()
        .find(|report| report.mode == target_mode)
        .map(|report| report.state)
}

fn run_identity_section(
    session: &mut RawQuerySession,
    log: &mut JsonLog,
) -> io::Result<EnvironmentSnapshot> {
    let snapshot = read_process_environment();
    let reply = session.query_fenced("identity:xtversion", XTVERSION_QUERY, log)?;
    let xtversion = parse_xtversion_reply(&reply.bytes);
    let json_line = format_identity_section(
        &snapshot,
        session.is_tty_available(),
        xtversion.as_deref(),
        reply.has_timed_out,
    );
    log.write_line(&json_line)?;
    Ok(snapshot)
}

fn run_mouse_modes_section(session: &mut RawQuerySession, log: &mut JsonLog) -> io::Result<()> {
    let decrpm_batch = build_mouse_decrpm_queries();
    let initial_reply = session.query_fenced("mouse_modes:initial_decrpm", &decrpm_batch, log)?;
    session.write_raw("mouse_modes:setup_m1", MOUSE_SETUP_M1, log)?;
    let after_m1_reply = session.query_fenced("mouse_modes:after_m1_decrpm", &decrpm_batch, log)?;
    session.write_raw("mouse_modes:teardown_m5", MOUSE_TEARDOWN_M5, log)?;
    let after_m5_reply = session.query_fenced("mouse_modes:after_m5_decrpm", &decrpm_batch, log)?;

    let has_timed_out =
        initial_reply.has_timed_out || after_m1_reply.has_timed_out || after_m5_reply.has_timed_out;
    let json_line = format_mouse_modes_section(
        &parse_mode_reports(&initial_reply.bytes),
        &parse_mode_reports(&after_m1_reply.bytes),
        &parse_mode_reports(&after_m5_reply.bytes),
        has_timed_out,
    );
    log.write_line(&json_line)
}

fn run_kitty_keyboard_section(
    session: &mut RawQuerySession,
    guard: &TerminalSessionGuard,
    log: &mut JsonLog,
) -> io::Result<()> {
    if !session.is_raw_query_supported() || session.has_fence_timed_out() {
        return log.write_line(&format_kitty_section(
            None,
            None,
            None,
            session.has_fence_timed_out(),
        ));
    }
    let initial_reply =
        session.query_fenced("kitty_keyboard:initial_query", KITTY_KEYBOARD_QUERY, log)?;
    // Mark Kitty pushed on the guard before writing `>1u` so any panic/error pops the stack.
    guard.set_kitty_pushed(true);
    let push_query = [KITTY_KEYBOARD_PUSH_DISAMBIGUATE, KITTY_KEYBOARD_QUERY].concat();
    let pushed_reply = session.query_fenced("kitty_keyboard:push_and_query", &push_query, log)?;
    let pop_query = [KITTY_KEYBOARD_POP, KITTY_KEYBOARD_QUERY].concat();
    let popped_reply = session.query_fenced("kitty_keyboard:pop_and_query", &pop_query, log)?;
    guard.set_kitty_pushed(false);

    let has_timed_out =
        initial_reply.has_timed_out || pushed_reply.has_timed_out || popped_reply.has_timed_out;
    let json_line = format_kitty_section(
        parse_kitty_flags_replies(&initial_reply.bytes)
            .first()
            .copied(),
        parse_kitty_flags_replies(&pushed_reply.bytes)
            .first()
            .copied(),
        parse_kitty_flags_replies(&popped_reply.bytes)
            .first()
            .copied(),
        has_timed_out,
    );
    log.write_line(&json_line)
}

fn probe_single_mode(
    session: &mut RawQuerySession,
    mode: u16,
    name: &'static str,
    enable_bytes: &[u8],
    disable_bytes: &[u8],
    log: &mut JsonLog,
) -> io::Result<(SingleModeProbe, bool)> {
    let decrpm = build_decrpm_query(mode);
    let enable_payload = [enable_bytes, &decrpm].concat();
    let enable_reply =
        session.query_fenced(&format!("modes:{name}:enable"), &enable_payload, log)?;
    let disable_payload = [disable_bytes, &decrpm].concat();
    let disable_reply =
        session.query_fenced(&format!("modes:{name}:disable"), &disable_payload, log)?;
    let probe_result = SingleModeProbe {
        mode,
        name,
        after_enable: find_report_state(&parse_mode_reports(&enable_reply.bytes), mode),
        after_disable: find_report_state(&parse_mode_reports(&disable_reply.bytes), mode),
    };
    Ok((
        probe_result,
        enable_reply.has_timed_out || disable_reply.has_timed_out,
    ))
}

fn run_modes_section(session: &mut RawQuerySession, log: &mut JsonLog) -> io::Result<()> {
    let specs: [(u16, &'static str, &[u8], &[u8]); 3] = [
        (
            2004,
            "bracketed_paste",
            BRACKETED_PASTE_ON,
            BRACKETED_PASTE_OFF,
        ),
        (1004, "focus_events", FOCUS_EVENTS_ON, FOCUS_EVENTS_OFF),
        (2026, "synchronized_output", SYNC_OUTPUT_ON, SYNC_OUTPUT_OFF),
    ];
    let mut entries = Vec::with_capacity(specs.len());
    let mut has_any_timed_out = false;
    for (mode, name, enable_bytes, disable_bytes) in specs {
        let (entry, has_timed_out) =
            probe_single_mode(session, mode, name, enable_bytes, disable_bytes, log)?;
        has_any_timed_out |= has_timed_out;
        entries.push(entry);
    }
    log.write_line(&format_modes_section(&entries, has_any_timed_out))
}

fn run_cell_size_section(session: &mut RawQuerySession, log: &mut JsonLog) -> io::Result<()> {
    let reply = session.query_fenced("cell_size:query", CELL_SIZE_PIXELS_QUERY, log)?;
    let cell_size = parse_cell_pixel_size(&reply.bytes);
    log.write_line(&format_cell_size_section(cell_size, reply.has_timed_out))
}

fn run_widths_section(session: &mut RawQuerySession, log: &mut JsonLog) -> io::Result<()> {
    let mut measurements = Vec::with_capacity(WIDTH_SAMPLES.len());
    let mut has_any_timed_out = false;
    for sample in WIDTH_SAMPLES {
        let (cells, has_timed_out) = if has_any_timed_out {
            (None, true)
        } else {
            session.measure_sample_width(sample, log)?
        };
        has_any_timed_out |= has_timed_out;
        measurements.push(MeasuredWidthSample {
            name: sample.name,
            category: sample.category,
            cells,
        });
    }
    log.write_line(&format_widths_section(&measurements, has_any_timed_out))
}

fn run_osc52_section(
    session: &mut RawQuerySession,
    args: &ProbeArgs,
    is_tmux: bool,
    log: &mut JsonLog,
) -> io::Result<()> {
    let Some(token) = &args.osc52_token else {
        return log.write_line(&format_osc52_section(false, false, false));
    };
    let plain_bytes = build_osc52_plain(token.as_bytes());
    session.write_raw("osc52:plain", &plain_bytes, log)?;
    if is_tmux {
        let tmux_bytes = build_osc52_tmux(token.as_bytes());
        session.write_raw("osc52:tmux", &tmux_bytes, log)?;
    }
    log.write_line(&format_osc52_section(true, true, is_tmux))
}

/// Runs all deterministic capability probe sections (`identity` through `background_color`).
///
/// # Errors
///
/// Returns an [`io::Error`] if terminal I/O or JSONL logging fails.
pub fn run_all_probe_sections(
    mut session: RawQuerySession,
    guard: &TerminalSessionGuard,
    args: &ProbeArgs,
    log: &mut JsonLog,
) -> io::Result<(TerminalWriter, ArmedColorListener)> {
    let env_snapshot = run_identity_section(&mut session, log)?;
    run_mouse_modes_section(&mut session, log)?;
    run_kitty_keyboard_section(&mut session, guard, log)?;
    run_modes_section(&mut session, log)?;
    run_cell_size_section(&mut session, log)?;
    run_widths_section(&mut session, log)?;
    run_osc52_section(&mut session, args, env_snapshot.is_tmux, log)?;

    // Drop the demand-driven raw reader before `crossterm::event` opens `/dev/tty`.
    let mut writer = session.into_writer();
    let color_listener = run_background_color_section(&mut writer, args.is_batch, log)?;
    Ok((writer, color_listener))
}
