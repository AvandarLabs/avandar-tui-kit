//! Deterministic capability probe section execution (`identity` through `background_color`).

use std::io;

use crate::armed_color::{ArmedColorListener, run_background_color_section};
use crate::environment::{EnvironmentSnapshot, read_process_environment};
use crate::json_log::JsonLog;
use crate::parsers::{
    latest_mode_state, parse_cell_pixel_size, parse_kitty_flags_replies, parse_mode_reports,
    parse_xtversion_reply,
};
use crate::probe_args::{Osc52Form, ProbeArgs};
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
    if !session.is_raw_query_supported() || session.has_fence_timed_out() {
        let json_line = format_mouse_modes_section(&[], &[], &[], session.has_fence_timed_out());
        return log.write_line(&json_line);
    }
    let decrpm_batch: Vec<u8> = [1000_u16, 1002, 1003, 1006]
        .into_iter()
        .flat_map(build_decrpm_query)
        .collect();
    let initial_reply = session.query_fenced("mouse_modes:initial_decrpm", &decrpm_batch, log)?;
    let initial_reports = parse_mode_reports(&initial_reply.bytes);
    if initial_reply.has_timed_out {
        return log.write_line(&format_mouse_modes_section(
            &initial_reports,
            &[],
            &[],
            true,
        ));
    }
    session.write_raw("mouse_modes:setup_m1", MOUSE_SETUP_M1, log)?;
    let after_m1_reply = session.query_fenced("mouse_modes:after_m1_decrpm", &decrpm_batch, log)?;
    // Always write `MOUSE_TEARDOWN_M5` via `write_raw` once `MOUSE_SETUP_M1` has been emitted.
    session.write_raw("mouse_modes:teardown_m5", MOUSE_TEARDOWN_M5, log)?;
    let after_m5_reply = session.query_fenced("mouse_modes:after_m5_decrpm", &decrpm_batch, log)?;

    let has_timed_out = after_m1_reply.has_timed_out || after_m5_reply.has_timed_out;
    let json_line = format_mouse_modes_section(
        &initial_reports,
        &parse_mode_reports(&after_m1_reply.bytes),
        &parse_mode_reports(&after_m5_reply.bytes),
        has_timed_out,
    );
    log.write_line(&json_line)
}

fn push_and_pop_kitty_flags(
    session: &mut RawQuerySession,
    guard: &TerminalSessionGuard,
    log: &mut JsonLog,
) -> io::Result<(Option<u16>, Option<u16>, bool)> {
    // Mark Kitty pushed on the guard before writing `>1u` so any panic/error pops the stack,
    // and emit `<u` via `write_raw` so a timeout on `pushed_query` never skips popping.
    guard.set_kitty_pushed(true);
    session.write_raw("kitty_keyboard:push", KITTY_KEYBOARD_PUSH_DISAMBIGUATE, log)?;
    let pushed_reply =
        session.query_fenced("kitty_keyboard:pushed_query", KITTY_KEYBOARD_QUERY, log)?;
    session.write_raw("kitty_keyboard:pop", KITTY_KEYBOARD_POP, log)?;
    guard.set_kitty_pushed(false);
    let popped_reply =
        session.query_fenced("kitty_keyboard:popped_query", KITTY_KEYBOARD_QUERY, log)?;
    Ok((
        parse_kitty_flags_replies(&pushed_reply.bytes)
            .first()
            .copied(),
        parse_kitty_flags_replies(&popped_reply.bytes)
            .first()
            .copied(),
        pushed_reply.has_timed_out || popped_reply.has_timed_out,
    ))
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
    let initial_flags = parse_kitty_flags_replies(&initial_reply.bytes)
        .first()
        .copied();
    if initial_reply.has_timed_out {
        return log.write_line(&format_kitty_section(initial_flags, None, None, true));
    }
    let (pushed_flags, popped_flags, has_timed_out) =
        push_and_pop_kitty_flags(session, guard, log)?;
    log.write_line(&format_kitty_section(
        initial_flags,
        pushed_flags,
        popped_flags,
        has_timed_out,
    ))
}

fn probe_single_mode(
    session: &mut RawQuerySession,
    mode: u16,
    name: &'static str,
    enable_bytes: &[u8],
    disable_bytes: &[u8],
    log: &mut JsonLog,
) -> io::Result<(SingleModeProbe, bool)> {
    if !session.is_raw_query_supported() || session.has_fence_timed_out() {
        return Ok((
            SingleModeProbe {
                mode,
                name,
                after_enable: None,
                after_disable: None,
            },
            session.has_fence_timed_out(),
        ));
    }
    let decrpm = build_decrpm_query(mode);
    session.write_raw(&format!("modes:{name}:enable"), enable_bytes, log)?;
    let enable_reply =
        session.query_fenced(&format!("modes:{name}:enable_decrpm"), &decrpm, log)?;
    // Always write `disable_bytes` via `write_raw` so a fence timeout on `enable_decrpm` never
    // leaves bracketed paste, focus reporting, or synchronized output enabled.
    session.write_raw(&format!("modes:{name}:disable"), disable_bytes, log)?;
    let disable_reply =
        session.query_fenced(&format!("modes:{name}:disable_decrpm"), &decrpm, log)?;
    let probe_result = SingleModeProbe {
        mode,
        name,
        after_enable: latest_mode_state(&parse_mode_reports(&enable_reply.bytes), mode),
        after_disable: latest_mode_state(&parse_mode_reports(&disable_reply.bytes), mode),
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
    let (should_send_plain, should_send_tmux) = match args.osc52_form {
        Osc52Form::Auto => (true, is_tmux),
        Osc52Form::Plain => (true, false),
        Osc52Form::Tmux => (false, true),
        Osc52Form::Both => (true, true),
    };
    if should_send_plain {
        let plain_bytes = build_osc52_plain(token.as_bytes());
        session.write_raw("osc52:plain", &plain_bytes, log)?;
    }
    if should_send_tmux {
        let tmux_bytes = build_osc52_tmux(token.as_bytes());
        session.write_raw("osc52:tmux", &tmux_bytes, log)?;
    }
    log.write_line(&format_osc52_section(
        true,
        should_send_plain,
        should_send_tmux,
    ))
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
    let environment_snapshot = run_identity_section(&mut session, log)?;
    run_mouse_modes_section(&mut session, log)?;
    run_kitty_keyboard_section(&mut session, guard, log)?;
    run_modes_section(&mut session, log)?;
    run_cell_size_section(&mut session, log)?;
    run_widths_section(&mut session, log)?;
    run_osc52_section(&mut session, args, environment_snapshot.is_tmux, log)?;

    // Drop the demand-driven raw reader before `crossterm::event` opens `/dev/tty`.
    let mut writer = session.into_writer();
    let color_listener = run_background_color_section(&mut writer, args.is_batch, log)?;
    Ok((writer, color_listener))
}

#[cfg(test)]
mod tests;
