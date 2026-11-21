//! Pure `crossterm::event::Event` state machine for `OSC 11` background color replies.
//!
//! On Windows ConPTY and Unix TTYs alike, `crossterm` decodes an `OSC 11` terminal
//! reply (`ESC ] 11 ; rgb : RRRR / GGGG / BBBB BEL` or `... ST`) into a stream of
//! [`Event::Key`] records. [`ColorReplyParser`] recognizes valid replies while
//! replaying non-matching prefixes in exact arrival order so genuine user `Esc`
//! or `Alt+]` input is never lost.

mod luminance;
#[cfg(test)]
mod tests;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use self::luminance::scale_hex_channel;
pub use self::luminance::{DARK_LUMINANCE_THRESHOLD, relative_luminance};

/// Blocking startup budget (in milliseconds) to wait for an `OSC 11` reply before the first frame.
pub const INITIAL_COLOR_WAIT_MS: u64 = 200;

/// Post-query armed window (in milliseconds) during which a late `OSC 11` reply is still swallowed.
pub const ARMED_COLOR_WINDOW_MS: u64 = 2_000;

const OSC_11_PREFIX_CHARS: [char; 7] = ['1', '1', ';', 'r', 'g', 'b', ':'];

/// Parsed terminal background color from an `OSC 11` (`rgb:R/G/B`) reply.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorReply {
    /// Scaled 8-bit red channel (`0..=255`).
    pub r: u8,
    /// Scaled 8-bit green channel (`0..=255`).
    pub g: u8,
    /// Scaled 8-bit blue channel (`0..=255`).
    pub b: u8,
    /// Raw 1-to-4-character ASCII hex strings received for red, green, and blue.
    pub channels: [String; 3],
    /// WCAG 2.x relative luminance in `0.0..=1.0`.
    pub luminance: f64,
    /// Whether `luminance` falls below [`DARK_LUMINANCE_THRESHOLD`].
    pub is_dark: bool,
}

/// Single-step outcome of feeding one [`Event`] into [`ColorReplyParser`].
#[derive(Debug, Clone, PartialEq)]
pub enum ColorStep {
    /// Parser was idle and `Event` did not begin an `OSC 11` sequence.
    Idle(Event),
    /// Event was consumed into the in-progress candidate `OSC 11` buffer.
    Buffering,
    /// Complete valid `OSC 11` reply was decoded and the candidate buffer was cleared.
    Complete(ColorReply),
    /// Candidate prefix failed to match `OSC 11`; returns buffered events plus the failing event in arrival order.
    Rejected(Vec<Event>),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    #[default]
    Idle,
    ExpectingOscBracket,
    ExpectingPrefix(usize),
    ReadingChannel(usize),
    ExpectingStBackslash,
}

/// Pure incremental parser that recognizes `OSC 11` background replies over [`crossterm::event::Event`] streams.
#[derive(Debug, Default, Clone)]
pub struct ColorReplyParser {
    state: ParserState,
    buffered_events: Vec<Event>,
    channels: [String; 3],
}

impl ColorReplyParser {
    /// Creates an idle [`ColorReplyParser`] with an empty replay buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: ParserState::Idle,
            buffered_events: Vec::new(),
            channels: [String::new(), String::new(), String::new()],
        }
    }

    /// Advances the parser with a single [`Event`] from `crossterm`.
    #[must_use]
    pub fn feed(&mut self, event: Event) -> ColorStep {
        if self.state == ParserState::Idle {
            return self.feed_idle(event);
        }
        if matches!(&event, Event::Key(key_event) if key_event.kind == KeyEventKind::Release) {
            return ColorStep::Buffering;
        }
        let Event::Key(key_event) = event else {
            return self.reject_with(event);
        };
        if key_event.kind != KeyEventKind::Press {
            return self.reject_with(Event::Key(key_event));
        }
        match self.state {
            ParserState::Idle => ColorStep::Idle(Event::Key(key_event)),
            ParserState::ExpectingOscBracket => self.feed_osc_bracket(key_event),
            ParserState::ExpectingPrefix(prefix_index) => self.feed_prefix(prefix_index, key_event),
            ParserState::ReadingChannel(channel_index) => {
                self.feed_channel(channel_index, key_event)
            }
            ParserState::ExpectingStBackslash => self.feed_st_backslash(key_event),
        }
    }

    /// Resets an in-progress sequence on timeout and returns buffered events in arrival order.
    #[must_use]
    pub fn flush_incomplete(&mut self) -> Vec<Event> {
        self.take_and_reset()
    }

    fn feed_idle(&mut self, event: Event) -> ColorStep {
        if let Event::Key(key_event) = &event
            && key_event.kind == KeyEventKind::Press
        {
            if key_event.code == KeyCode::Char(']') && key_event.modifiers == KeyModifiers::ALT {
                self.buffered_events.push(event);
                self.state = ParserState::ExpectingPrefix(0);
                return ColorStep::Buffering;
            }
            if key_event.code == KeyCode::Esc && key_event.modifiers == KeyModifiers::NONE {
                self.buffered_events.push(event);
                self.state = ParserState::ExpectingOscBracket;
                return ColorStep::Buffering;
            }
        }
        ColorStep::Idle(event)
    }

    fn feed_osc_bracket(&mut self, key_event: KeyEvent) -> ColorStep {
        if key_event.code == KeyCode::Char(']') && key_event.modifiers == KeyModifiers::NONE {
            self.buffered_events.push(Event::Key(key_event));
            self.state = ParserState::ExpectingPrefix(0);
            ColorStep::Buffering
        } else {
            self.reject_with(Event::Key(key_event))
        }
    }

    fn feed_prefix(&mut self, prefix_index: usize, key_event: KeyEvent) -> ColorStep {
        let expected_char = OSC_11_PREFIX_CHARS[prefix_index];
        if key_event.code == KeyCode::Char(expected_char)
            && key_event.modifiers == KeyModifiers::NONE
        {
            self.buffered_events.push(Event::Key(key_event));
            self.state = if prefix_index + 1 < OSC_11_PREFIX_CHARS.len() {
                ParserState::ExpectingPrefix(prefix_index + 1)
            } else {
                ParserState::ReadingChannel(0)
            };
            ColorStep::Buffering
        } else {
            self.reject_with(Event::Key(key_event))
        }
    }

    fn feed_channel(&mut self, channel_index: usize, key_event: KeyEvent) -> ColorStep {
        if let Some(hex_char) = extract_hex_digit(key_event) {
            if self.channels[channel_index].len() < 4 {
                self.channels[channel_index].push(hex_char);
                self.buffered_events.push(Event::Key(key_event));
                return ColorStep::Buffering;
            }
            return self.reject_with(Event::Key(key_event));
        }

        let has_channel_digits = !self.channels[channel_index].is_empty();
        if has_channel_digits
            && channel_index < 2
            && key_event.code == KeyCode::Char('/')
            && key_event.modifiers == KeyModifiers::NONE
        {
            self.buffered_events.push(Event::Key(key_event));
            self.state = ParserState::ReadingChannel(channel_index + 1);
            return ColorStep::Buffering;
        }

        if has_channel_digits && channel_index == 2 {
            if is_single_event_terminator(key_event) {
                return self.complete_reply();
            }
            if key_event.code == KeyCode::Esc && key_event.modifiers == KeyModifiers::NONE {
                self.buffered_events.push(Event::Key(key_event));
                self.state = ParserState::ExpectingStBackslash;
                return ColorStep::Buffering;
            }
        }

        self.reject_with(Event::Key(key_event))
    }

    fn feed_st_backslash(&mut self, key_event: KeyEvent) -> ColorStep {
        if key_event.code == KeyCode::Char('\\') && key_event.modifiers == KeyModifiers::NONE {
            self.complete_reply()
        } else {
            self.reject_with(Event::Key(key_event))
        }
    }

    fn complete_reply(&mut self) -> ColorStep {
        let channels = std::mem::take(&mut self.channels);
        self.state = ParserState::Idle;
        self.buffered_events.clear();

        let red = scale_hex_channel(&channels[0]);
        let green = scale_hex_channel(&channels[1]);
        let blue = scale_hex_channel(&channels[2]);
        let luminance = relative_luminance(red, green, blue);

        ColorStep::Complete(ColorReply {
            r: red,
            g: green,
            b: blue,
            channels,
            luminance,
            is_dark: luminance < DARK_LUMINANCE_THRESHOLD,
        })
    }

    fn reject_with(&mut self, event: Event) -> ColorStep {
        self.buffered_events.push(event);
        ColorStep::Rejected(self.take_and_reset())
    }

    fn take_and_reset(&mut self) -> Vec<Event> {
        self.state = ParserState::Idle;
        for channel_hex in &mut self.channels {
            channel_hex.clear();
        }
        std::mem::take(&mut self.buffered_events)
    }
}

fn extract_hex_digit(key_event: KeyEvent) -> Option<char> {
    match key_event.code {
        KeyCode::Char(hex_char @ ('0'..='9' | 'a'..='f'))
            if key_event.modifiers == KeyModifiers::NONE =>
        {
            Some(hex_char)
        }
        KeyCode::Char(hex_char @ 'A'..='F')
            if key_event.modifiers == KeyModifiers::NONE
                || key_event.modifiers == KeyModifiers::SHIFT =>
        {
            Some(hex_char)
        }
        _ => None,
    }
}

fn is_single_event_terminator(key_event: KeyEvent) -> bool {
    (key_event.code == KeyCode::Char('g') && key_event.modifiers == KeyModifiers::CONTROL)
        || (key_event.code == KeyCode::Char('\x07') && key_event.modifiers == KeyModifiers::NONE)
        || (key_event.code == KeyCode::Char('\\') && key_event.modifiers == KeyModifiers::ALT)
}
