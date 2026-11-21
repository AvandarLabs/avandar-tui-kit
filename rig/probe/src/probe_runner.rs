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
pub use crate::session_guard::build_teardown_bytes;
use crate::terminal_io::{
    RawQuerySession, TerminalSessionGuard, count_leading_press_burst, drain_event_burst,
};

/// Human-readable description of the interactive clean-exit key chord.
pub const EXIT_CHORD_DESCRIPTION: &str = "Ctrl+C";

/// Human-readable description of the interactive test-panic trigger key.
pub const PANIC_KEY_DESCRIPTION: &str = "F12";

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

fn flush_expired_color_prefix(
    color_listener: &mut ArmedColorListener,
    now: Instant,
    log: &mut JsonLog,
) -> io::Result<LoopControl> {
    let expired_events = color_listener.expire_if_due(now);
    if expired_events.is_empty() {
        return Ok(LoopControl::Continue);
    }
    let burst_press_count = count_leading_press_burst(&expired_events);
    log_and_classify_events(&expired_events, burst_press_count, log)
}

fn process_burst_at(
    events: Vec<Event>,
    burst_press_count: usize,
    now: Instant,
    color_listener: &mut ArmedColorListener,
    log: &mut JsonLog,
) -> io::Result<LoopControl> {
    let was_armed = color_listener.is_armed();
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
        count_leading_press_burst(&user_events)
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
    let initial_burst = count_leading_press_burst(&initial_queued);
    if log_and_classify_events(&initial_queued, initial_burst, log)? == LoopControl::CleanExit {
        return Ok(());
    }
    loop {
        if flush_expired_color_prefix(&mut color_listener, Instant::now(), log)?
            == LoopControl::CleanExit
        {
            return Ok(());
        }
        if let Some(remaining) = color_listener.remaining_window(Instant::now())
            && !crossterm::event::poll(remaining)?
        {
            continue;
        }
        let first_event = crossterm::event::read()?;
        let (drained_events, burst_press_count) =
            drain_event_burst(first_event, poll_zero_crossterm)?;
        let control = process_burst_at(
            drained_events,
            burst_press_count,
            Instant::now(),
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
            EXIT_CHORD_DESCRIPTION,
            PANIC_KEY_DESCRIPTION,
        ))?;
        return log.mark_ready();
    }
    if !writer.is_tty_available() {
        return Err(io::Error::other(
            "interactive capability_probe requires a TTY; pass --batch for detached runs",
        ));
    }
    guard.enable_interactive_modes(&mut writer, args.is_push_kitty, &mut log)?;
    log.write_line(&format_summary_record(
        "interactive",
        EXIT_CHORD_DESCRIPTION,
        PANIC_KEY_DESCRIPTION,
    ))?;
    log.mark_ready()?;
    run_interactive_loop(color_listener, &mut log)
}

/// Parses `std::env::args()` and executes `capability_probe` in batch or interactive mode.
///
/// # Panics
///
/// Panics intentionally in interactive mode when the user presses `F12`, exercising the
/// panic-hook terminal teardown path.
#[must_use = "return the ExitCode from main so the process exit status is preserved"]
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
    use std::fs;

    use crossterm::event::KeyEvent;

    use super::*;
    use crate::color_reply::ColorReplyParser;
    use crate::sequences::{
        BRACKETED_PASTE_OFF, FOCUS_EVENTS_OFF, KITTY_KEYBOARD_POP, MOUSE_TEARDOWN_M5,
        SYNC_OUTPUT_OFF,
    };

    fn has_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn teardown_bytes_and_control_keys_cover_clean_exit_and_panic_paths() {
        // Teardown resets M5 mouse modes, paste, focus, sync, and pops kitty iff pushed.
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

    #[test]
    fn overdue_armed_listener_flushes_buffered_prefix_and_recounts_burst() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let mut listener =
            ArmedColorListener::armed(ColorReplyParser::new(), start, deadline, Vec::new());
        let esc_event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            listener.filter_event(esc_event, start + Duration::from_millis(1_990)),
            ArmedColorOutcome::Swallowed(None)
        );
        assert!(listener.is_armed());
        assert!(listener.remaining_window(deadline).is_none());

        let log_path = std::env::temp_dir().join(format!(
            "rig-probe-runner-overdue-{}.jsonl",
            std::process::id()
        ));
        let mut log = JsonLog::create(&log_path).expect("log must create");
        let next_key = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        let control = process_burst_at(vec![next_key], 1, deadline, &mut listener, &mut log)
            .expect("overdue burst must process cleanly");
        assert_eq!(control, LoopControl::Continue);
        assert!(!listener.is_armed());

        let logged = fs::read_to_string(&log_path).expect("log must read");
        let _ = fs::remove_file(&log_path);
        assert!(logged.contains(r#""burst_press_count":2,"event_type":"key","code":"esc""#));
        assert!(logged.contains(r#""burst_press_count":2,"event_type":"key","code":"char:x""#));
    }

    #[test]
    fn flush_expired_color_prefix_logs_buffered_esc_before_blocking_read() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let mut listener =
            ArmedColorListener::armed(ColorReplyParser::new(), start, deadline, Vec::new());
        let esc_event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            listener.filter_event(esc_event, start + Duration::from_millis(1_980)),
            ArmedColorOutcome::Swallowed(None)
        );

        let log_path = std::env::temp_dir().join(format!(
            "rig-probe-runner-flush-{}.jsonl",
            std::process::id()
        ));
        let mut log = JsonLog::create(&log_path).expect("log must create");
        let control = flush_expired_color_prefix(
            &mut listener,
            deadline + Duration::from_millis(5),
            &mut log,
        )
        .expect("overdue flush must succeed");
        assert_eq!(control, LoopControl::Continue);
        assert!(!listener.is_armed());

        let logged = fs::read_to_string(&log_path).expect("log must read");
        let _ = fs::remove_file(&log_path);
        assert!(logged.contains(r#""burst_press_count":1,"event_type":"key","code":"esc""#));
    }
}
