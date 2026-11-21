//! Pure and zero-timeout event-burst drainer for interactive key/paste streams.

use std::io;

use crossterm::event::{Event, KeyEventKind};

pub fn count_leading_press_burst(events: &[Event]) -> usize {
    let mut burst_press_count = 0;
    for event in events {
        match event {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                burst_press_count += 1;
            }
            // Windows ConPTY interleaves `Release` after each pasted `Press`; skip `Release`
            // so the leading press burst count matches the pasted character count.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Release => {}
            _ => break,
        }
    }
    burst_press_count
}

/// Drains immediately available events after `first_event` and counts leading key-press events.
///
/// # Errors
///
/// Propagates any [`io::Error`] returned by `poll_zero`.
pub fn drain_event_burst<F>(first_event: Event, mut poll_zero: F) -> io::Result<(Vec<Event>, usize)>
where
    F: FnMut() -> io::Result<Option<Event>>,
{
    let mut events = vec![first_event];
    while let Some(next_event) = poll_zero()? {
        events.push(next_event);
    }
    let burst_press_count = count_leading_press_burst(&events);
    Ok((events, burst_press_count))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    #[test]
    fn drain_event_burst_counts_consecutive_press_keys_before_trailing_non_key_event() {
        let mut queued = VecDeque::from([
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::FocusLost)),
            Ok(None),
        ]);
        let first_event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let (events, burst_len) =
            drain_event_burst(first_event, || queued.pop_front().unwrap_or(Ok(None)))
                .expect("burst drain must succeed");
        assert_eq!(burst_len, 3);
        assert_eq!(events.len(), 4);
        assert_eq!(events[3], Event::FocusLost);
    }

    #[test]
    fn drain_event_burst_skips_interleaved_windows_key_releases() {
        let mut windows_queued = VecDeque::from([
            Ok(Some(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )))),
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )))),
            Ok(Some(Event::Resize(80, 24))),
            Ok(None),
        ]);
        let first_press = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let (events, burst_len) = drain_event_burst(first_press, || {
            windows_queued.pop_front().unwrap_or(Ok(None))
        })
        .expect("windows burst drain must succeed");
        assert_eq!(burst_len, 2);
        assert_eq!(events.len(), 5);
    }

    #[test]
    fn drain_event_burst_returns_zero_press_count_when_first_event_is_non_key() {
        let (focus_events, focus_burst) = drain_event_burst(Event::FocusGained, || Ok(None))
            .expect("single non-key event must succeed");
        assert_eq!(focus_burst, 0);
        assert_eq!(focus_events, vec![Event::FocusGained]);
    }

    #[test]
    fn drain_event_burst_propagates_poll_error() {
        assert!(drain_event_burst(Event::FocusLost, || Err(io::Error::other("poll"))).is_err());
    }
}
