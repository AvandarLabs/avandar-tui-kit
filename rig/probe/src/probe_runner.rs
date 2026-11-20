//! Top-level `capability_probe` runner, teardown byte builder, and interactive control keys.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::armed_color::{ArmedColorListener, ArmedColorOutcome};
use crate::event_json::format_event_record;
use crate::json_log::JsonLog;
use crate::probe_args::{ProbeArgs, parse_probe_args};
use crate::probe_sections::run_all_probe_sections;
use crate::section_json::{format_background_color_section, format_summary_record};
use crate::sequences::{
    BRACKETED_PASTE_OFF, BRACKETED_PASTE_ON, FOCUS_EVENTS_OFF, FOCUS_EVENTS_ON, KITTY_KEYBOARD_POP,
    KITTY_KEYBOARD_PUSH_DISAMBIGUATE, MOUSE_TEARDOWN_M5, SYNC_OUTPUT_OFF,
};
use crate::terminal_io::{
    RawQuerySession, TerminalSessionGuard, TerminalWriter, drain_event_burst,
};

/// Human-readable description of the interactive clean-exit key chord.
pub const EXIT_CHORD_DESC: &str = "Ctrl+C";

/// Human-readable description of the interactive test-panic trigger key.
pub const PANIC_KEY_DESC: &str = "F12";

/// Control action chosen for an interactive [`Event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopControl {
    /// Keep running the interactive event loop.
    Continue,
    /// Exit the interactive loop cleanly (`Ctrl+C`), running `Drop` teardown.
    CleanExit,
    /// Trigger an intentional test panic (`F12`), exercising panic-hook teardown.
    TriggerPanic,
}

/// Classifies whether `event` requests clean exit (`Ctrl+C`), test panic (`F12`), or continuation.
#[must_use]
pub fn classify_probe_control(event: &Event) -> LoopControl {
    let Event::Key(key_event) = event else {
        return LoopControl::Continue;
    };
    if key_event.kind != KeyEventKind::Press {
        return LoopControl::Continue;
    }
    match key_event.code {
        KeyCode::Char('c' | 'C') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
            LoopControl::CleanExit
        }
        KeyCode::F(12) if key_event.modifiers == KeyModifiers::NONE => LoopControl::TriggerPanic,
        _ => LoopControl::Continue,
    }
}

/// Builds the full terminal-mode reset sequence for `Drop` and panic-hook teardown.
#[must_use]
pub fn build_teardown_bytes(was_kitty_pushed: bool) -> Vec<u8> {
    let mut teardown = Vec::with_capacity(64);
    teardown.extend_from_slice(MOUSE_TEARDOWN_M5);
    teardown.extend_from_slice(BRACKETED_PASTE_OFF);
    teardown.extend_from_slice(FOCUS_EVENTS_OFF);
    teardown.extend_from_slice(SYNC_OUTPUT_OFF);
    if was_kitty_pushed {
        teardown.extend_from_slice(KITTY_KEYBOARD_POP);
    }
    teardown
}

fn enable_interactive_modes(
    guard: &TerminalSessionGuard,
    writer: &mut TerminalWriter,
    is_push_kitty: bool,
    log: &mut JsonLog,
) -> io::Result<()> {
    guard.enable_interactive_mouse(writer, log)?;
    writer.write_logged("interactive:bracketed_paste_on", BRACKETED_PASTE_ON, log)?;
    writer.write_logged("interactive:focus_events_on", FOCUS_EVENTS_ON, log)?;
    if is_push_kitty {
        guard.set_kitty_pushed(true);
        writer.write_logged(
            "interactive:kitty_push",
            KITTY_KEYBOARD_PUSH_DISAMBIGUATE,
            log,
        )?;
    }
    Ok(())
}

fn log_and_classify_events(
    events: &[Event],
    burst_press_count: usize,
    log: &mut JsonLog,
) -> io::Result<LoopControl> {
    for event in events {
        log.write_line(&format_event_record(event, burst_press_count))?;
        match classify_probe_control(event) {
            LoopControl::Continue => {}
            LoopControl::CleanExit => return Ok(LoopControl::CleanExit),
            LoopControl::TriggerPanic => panic!("capability_probe triggered test panic"),
        }
    }
    Ok(LoopControl::Continue)
}

fn process_burst_with_color_filter(
    events: Vec<Event>,
    burst_press_count: usize,
    color_listener: &mut ArmedColorListener,
    log: &mut JsonLog,
) -> io::Result<LoopControl> {
    let now = Instant::now();
    let was_armed = color_listener.remaining_window(now).is_some();
    let mut user_events = Vec::with_capacity(events.len());
    for event in events {
        match color_listener.filter_event(event, now) {
            ArmedColorOutcome::Swallowed(Some((late_reply, elapsed_ms))) => {
                let line = format_background_color_section(
                    "background_color_late",
                    "late",
                    elapsed_ms,
                    Some(&late_reply),
                );
                log.write_line(&line)?;
            }
            ArmedColorOutcome::Swallowed(None) => {}
            ArmedColorOutcome::ForwardEvents(forwarded) => user_events.extend(forwarded),
        }
    }
    let effective_burst_count = if was_armed {
        user_events
            .iter()
            .take_while(|item| matches!(item, Event::Key(_)))
            .filter(|item| matches!(item, Event::Key(key) if key.kind == KeyEventKind::Press))
            .count()
    } else {
        burst_press_count
    };
    log_and_classify_events(&user_events, effective_burst_count, log)
}

fn poll_zero_crossterm() -> io::Result<Option<Event>> {
    if crossterm::event::poll(Duration::ZERO)? {
        Ok(Some(crossterm::event::read()?))
    } else {
        Ok(None)
    }
}

fn run_interactive_loop(
    mut color_listener: ArmedColorListener,
    log: &mut JsonLog,
) -> io::Result<()> {
    let initial_queued = color_listener.take_queued_events();
    if log_and_classify_events(&initial_queued, 0, log)? == LoopControl::CleanExit {
        return Ok(());
    }
    loop {
        if let Some(remaining) = color_listener.remaining_window(Instant::now())
            && !crossterm::event::poll(remaining)?
        {
            let flushed_events = color_listener.expire_if_due(Instant::now());
            if log_and_classify_events(&flushed_events, 0, log)? == LoopControl::CleanExit {
                return Ok(());
            }
            continue;
        }
        let first_event = crossterm::event::read()?;
        let (drained_events, burst_press_count) =
            drain_event_burst(first_event, poll_zero_crossterm)?;
        let control = process_burst_with_color_filter(
            drained_events,
            burst_press_count,
            &mut color_listener,
            log,
        )?;
        if control == LoopControl::CleanExit {
            return Ok(());
        }
    }
}

fn execute_probe(args: &ProbeArgs) -> io::Result<()> {
    let mut log = JsonLog::create(&args.log_path)?;
    let session = RawQuerySession::open();
    let guard = TerminalSessionGuard::acquire(session.is_tty_available())?;
    let (mut writer, color_listener) = run_all_probe_sections(session, &guard, args, &mut log)?;
    if args.is_batch {
        log.write_line(&format_summary_record(
            "batch",
            EXIT_CHORD_DESC,
            PANIC_KEY_DESC,
        ))?;
        return log.mark_ready();
    }
    if !writer.is_tty_available() {
        return Err(io::Error::other(
            "interactive capability_probe requires a TTY; pass --batch for detached runs",
        ));
    }
    enable_interactive_modes(&guard, &mut writer, args.is_push_kitty, &mut log)?;
    log.write_line(&format_summary_record(
        "interactive",
        EXIT_CHORD_DESC,
        PANIC_KEY_DESC,
    ))?;
    log.mark_ready()?;
    run_interactive_loop(color_listener, &mut log)
}

/// Parses `std::env::args()` and executes `capability_probe` in batch or interactive mode.
#[must_use]
pub fn run_from_env() -> ExitCode {
    let parsed_args = match parse_probe_args(std::env::args().skip(1)) {
        Ok(valid_args) => valid_args,
        Err(usage_error) => {
            let _ = writeln!(
                io::stderr(),
                "capability_probe argument error: {usage_error}"
            );
            return ExitCode::from(2);
        }
    };
    if let Err(runtime_error) = execute_probe(&parsed_args) {
        let _ = writeln!(io::stderr(), "capability_probe failed: {runtime_error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyEvent;

    use super::*;

    fn has_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn teardown_bytes_and_control_keys_cover_clean_exit_and_panic_paths() {
        // Review Focus #4: teardown resets M5 mouse modes, paste, focus, sync, and pops kitty iff pushed.
        let with_kitty = build_teardown_bytes(true);
        for required in [
            MOUSE_TEARDOWN_M5,
            BRACKETED_PASTE_OFF,
            FOCUS_EVENTS_OFF,
            SYNC_OUTPUT_OFF,
            KITTY_KEYBOARD_POP,
        ] {
            assert!(has_subslice(&with_kitty, required));
        }

        let without_kitty = build_teardown_bytes(false);
        assert!(has_subslice(&without_kitty, MOUSE_TEARDOWN_M5));
        assert!(has_subslice(&without_kitty, BRACKETED_PASTE_OFF));
        assert!(has_subslice(&without_kitty, FOCUS_EVENTS_OFF));
        assert!(has_subslice(&without_kitty, SYNC_OUTPUT_OFF));
        assert!(!has_subslice(&without_kitty, KITTY_KEYBOARD_POP));

        let ctrl_c_press = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let ctrl_c_release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ));
        let f12_press = Event::Key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        let plain_key = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

        assert_eq!(
            classify_probe_control(&ctrl_c_press),
            LoopControl::CleanExit
        );
        assert_eq!(
            classify_probe_control(&ctrl_c_release),
            LoopControl::Continue
        );
        assert_eq!(
            classify_probe_control(&f12_press),
            LoopControl::TriggerPanic
        );
        assert_eq!(classify_probe_control(&plain_key), LoopControl::Continue);
    }
}
