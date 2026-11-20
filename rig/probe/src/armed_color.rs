//! Startup `OSC 11` background color polling and 2-second armed interactive listener.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::Event;

use crate::color_reply::{
    ARMED_COLOR_WINDOW_MS, ColorReply, ColorReplyParser, ColorStep, INITIAL_COLOR_WAIT_MS,
};
use crate::json_log::JsonLog;
use crate::section_json::format_background_color_section;
use crate::sequences::OSC_11_BACKGROUND_QUERY;
use crate::session_guard::TerminalWriter;

fn elapsed_millis(started_at: Instant, now: Instant) -> u64 {
    u64::try_from(now.saturating_duration_since(started_at).as_millis()).unwrap_or(u64::MAX)
}

/// Outcome of filtering an interactive event through an armed `OSC 11` listener.
#[derive(Debug, Clone, PartialEq)]
pub enum ArmedColorOutcome {
    /// Event was buffered or swallowed as part of an `OSC 11` reply; optional completed reply returned.
    Swallowed(Option<(ColorReply, u64)>),
    /// Event(s) did not belong to an `OSC 11` reply and must be logged as normal user input.
    ForwardEvents(Vec<Event>),
}

/// Post-query `OSC 11` listener that stays armed up to 2 seconds in interactive mode.
#[derive(Debug)]
pub struct ArmedColorListener {
    parser: ColorReplyParser,
    query_started_at: Instant,
    armed_deadline: Option<Instant>,
    queued_events: Vec<Event>,
}

impl ArmedColorListener {
    /// Creates an unarmed listener carrying any non-color events captured during initial polling.
    #[must_use]
    pub fn unarmed(queued_events: Vec<Event>) -> Self {
        Self {
            parser: ColorReplyParser::new(),
            query_started_at: Instant::now(),
            armed_deadline: None,
            queued_events,
        }
    }

    /// Creates an armed listener that continues watching for a late `OSC 11` reply until `deadline`.
    #[must_use]
    pub const fn armed(
        parser: ColorReplyParser,
        query_started_at: Instant,
        deadline: Instant,
        queued_events: Vec<Event>,
    ) -> Self {
        Self {
            parser,
            query_started_at,
            armed_deadline: Some(deadline),
            queued_events,
        }
    }

    /// Takes any non-color events already collected during the initial `200 ms` poll window.
    pub fn take_queued_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.queued_events)
    }

    /// Returns the remaining duration until the armed `OSC 11` window expires, if still armed.
    #[must_use]
    pub fn remaining_window(&self, now: Instant) -> Option<Duration> {
        self.armed_deadline
            .and_then(|deadline| deadline.checked_duration_since(now))
    }

    /// Expires the armed window at or after `now`, returning any buffered incomplete prefix events.
    pub fn expire_if_due(&mut self, now: Instant) -> Vec<Event> {
        let Some(deadline) = self.armed_deadline else {
            return Vec::new();
        };
        if now < deadline {
            return Vec::new();
        }
        self.armed_deadline = None;
        self.parser.flush_incomplete()
    }

    /// Feeds one interactive [`Event`] at timestamp `now` while armed, or forwards immediately if unarmed.
    pub fn filter_event(&mut self, event: Event, now: Instant) -> ArmedColorOutcome {
        let mut released_events = self.expire_if_due(now);
        if self.armed_deadline.is_none() {
            released_events.push(event);
            return ArmedColorOutcome::ForwardEvents(released_events);
        }
        match self.parser.feed(event) {
            ColorStep::Buffering => ArmedColorOutcome::Swallowed(None),
            ColorStep::Complete(reply) => {
                self.armed_deadline = None;
                let elapsed_ms = elapsed_millis(self.query_started_at, now);
                ArmedColorOutcome::Swallowed(Some((reply, elapsed_ms)))
            }
            ColorStep::Idle(single_event) => ArmedColorOutcome::ForwardEvents(vec![single_event]),
            ColorStep::Rejected(rejected_events) => {
                ArmedColorOutcome::ForwardEvents(rejected_events)
            }
        }
    }
}

fn poll_color_until(
    parser: &mut ColorReplyParser,
    queued_events: &mut Vec<Event>,
    deadline: Instant,
) -> io::Result<Option<ColorReply>> {
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !crossterm::event::poll(remaining)? {
            break;
        }
        match parser.feed(crossterm::event::read()?) {
            ColorStep::Complete(reply) => return Ok(Some(reply)),
            ColorStep::Buffering => {}
            ColorStep::Idle(non_color_event) => queued_events.push(non_color_event),
            ColorStep::Rejected(rejected_events) => queued_events.extend(rejected_events),
        }
    }
    Ok(None)
}

fn finish_background_color_window(
    mut parser: ColorReplyParser,
    mut queued_events: Vec<Event>,
    started_at: Instant,
    is_batch: bool,
    log: &mut JsonLog,
) -> io::Result<ArmedColorListener> {
    let armed_deadline = started_at + Duration::from_millis(ARMED_COLOR_WINDOW_MS);
    if !is_batch {
        let elapsed_ms = elapsed_millis(started_at, Instant::now());
        let line =
            format_background_color_section("background_color", "armed_pending", elapsed_ms, None);
        log.write_line(&line)?;
        return Ok(ArmedColorListener::armed(
            parser,
            started_at,
            armed_deadline,
            queued_events,
        ));
    }
    if let Some(late_reply) = poll_color_until(&mut parser, &mut queued_events, armed_deadline)? {
        let elapsed_ms = elapsed_millis(started_at, Instant::now());
        let line = format_background_color_section(
            "background_color",
            "late",
            elapsed_ms,
            Some(&late_reply),
        );
        log.write_line(&line)?;
    } else {
        queued_events.extend(parser.flush_incomplete());
        let elapsed_ms = elapsed_millis(started_at, Instant::now());
        let line = format_background_color_section("background_color", "timeout", elapsed_ms, None);
        log.write_line(&line)?;
    }
    Ok(ArmedColorListener::unarmed(queued_events))
}

/// Sends `OSC 11` and polls for the background color reply over `crossterm::event`.
///
/// # Errors
///
/// Returns an [`io::Error`] if writing the query, polling `crossterm`, or logging fails.
pub fn run_background_color_section(
    writer: &mut TerminalWriter,
    is_batch: bool,
    log: &mut JsonLog,
) -> io::Result<ArmedColorListener> {
    if !writer.is_tty_available() {
        let line = format_background_color_section("background_color", "skipped_no_tty", 0, None);
        log.write_line(&line)?;
        return Ok(ArmedColorListener::unarmed(Vec::new()));
    }
    while crossterm::event::poll(Duration::ZERO)? {
        let _ = crossterm::event::read()?;
    }
    let started_at = Instant::now();
    writer.write_logged("background_color:osc11_query", OSC_11_BACKGROUND_QUERY, log)?;
    let mut parser = ColorReplyParser::new();
    let mut queued_events = Vec::new();
    let initial_deadline = started_at + Duration::from_millis(INITIAL_COLOR_WAIT_MS);
    if let Some(reply) = poll_color_until(&mut parser, &mut queued_events, initial_deadline)? {
        let elapsed_ms = elapsed_millis(started_at, Instant::now());
        let line = format_background_color_section(
            "background_color",
            "initial",
            elapsed_ms,
            Some(&reply),
        );
        log.write_line(&line)?;
        return Ok(ArmedColorListener::unarmed(queued_events));
    }
    finish_background_color_window(parser, queued_events, started_at, is_batch, log)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    #[test]
    fn armed_color_listener_completes_late_reply_and_flushes_on_deadline() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let initial_focus = Event::FocusGained;
        let mut listener = ArmedColorListener::armed(
            ColorReplyParser::new(),
            start,
            deadline,
            vec![initial_focus.clone()],
        );
        assert_eq!(listener.take_queued_events(), vec![initial_focus]);
        assert!(
            listener
                .remaining_window(start + Duration::from_millis(500))
                .is_some()
        );

        let alt_bracket = Event::Key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT));
        assert_eq!(
            listener.filter_event(alt_bracket, start + Duration::from_millis(300)),
            ArmedColorOutcome::Swallowed(None)
        );
        for body_char in "11;rgb:1a/1b/26".chars() {
            let char_event =
                Event::Key(KeyEvent::new(KeyCode::Char(body_char), KeyModifiers::NONE));
            assert_eq!(
                listener.filter_event(char_event, start + Duration::from_millis(310)),
                ArmedColorOutcome::Swallowed(None)
            );
        }
        let bel_event = Event::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL));
        let outcome = listener.filter_event(bel_event, start + Duration::from_millis(350));
        let ArmedColorOutcome::Swallowed(Some((reply, elapsed_ms))) = outcome else {
            panic!("expected swallowed completed late reply, got {outcome:?}");
        };
        assert!(reply.is_dark);
        assert_eq!(elapsed_ms, 350);
        assert!(
            listener
                .remaining_window(start + Duration::from_millis(400))
                .is_none()
        );
    }

    #[test]
    fn armed_color_listener_flushes_buffered_esc_when_window_expires() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let mut listener =
            ArmedColorListener::armed(ColorReplyParser::new(), start, deadline, Vec::new());

        let esc_event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            listener.filter_event(esc_event.clone(), start + Duration::from_millis(1_950)),
            ArmedColorOutcome::Swallowed(None)
        );
        assert!(
            listener
                .expire_if_due(start + Duration::from_millis(1_999))
                .is_empty()
        );
        assert_eq!(listener.expire_if_due(deadline), vec![esc_event]);
        assert!(listener.remaining_window(deadline).is_none());
    }
}
