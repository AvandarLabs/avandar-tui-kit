//! Pure `crossterm::event::Event` state machine for `OSC 11` background color replies
//! and WCAG 2.x relative luminance math.
//!
//! On Windows ConPTY and Unix TTYs alike, `crossterm` decodes an `OSC 11` terminal
//! reply (`ESC ] 11 ; rgb : RRRR / GGGG / BBBB BEL` or `... ST`) into a stream of
//! [`Event::Key`] records. [`ColorReplyParser`] recognizes valid replies while
//! replaying non-matching prefixes in exact arrival order so genuine user `Esc`
//! or `Alt+]` input is never lost.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Blocking startup budget (in milliseconds) to wait for an `OSC 11` reply before the first frame.
pub const INITIAL_COLOR_WAIT_MS: u64 = 200;

/// Post-query armed window (in milliseconds) during which a late `OSC 11` reply is still swallowed.
pub const ARMED_COLOR_WINDOW_MS: u64 = 2_000;

/// WCAG equal-contrast relative luminance cutoff separating dark (`< 0.179`) and light (`>= 0.179`) backgrounds.
pub const DARK_LUMINANCE_THRESHOLD: f64 = 0.179;

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

fn scale_hex_channel(channel_hex: &str) -> u8 {
    let mut raw_value = 0_u32;
    let mut digit_count = 0_u32;
    for hex_char in channel_hex.chars() {
        if let Some(digit_value) = hex_char.to_digit(16) {
            raw_value = (raw_value << 4) | digit_value;
            digit_count += 1;
        }
    }
    if digit_count == 0 || digit_count > 4 {
        return 0;
    }
    let max_value = (1_u32 << (4 * digit_count)) - 1;
    let scaled_value = (raw_value * 255 + max_value / 2) / max_value;
    u8::try_from(scaled_value).unwrap_or(u8::MAX)
}

fn srgb_channel_to_linear(channel_value: u8) -> f64 {
    let srgb_normalized = f64::from(channel_value) / 255.0;
    if srgb_normalized <= 0.040_45 {
        srgb_normalized / 12.92
    } else {
        ((srgb_normalized + 0.055) / 1.055).powf(2.4)
    }
}

/// Computes WCAG 2.x sRGB relative luminance in `0.0..=1.0` for an 8-bit RGB triple.
#[must_use]
pub fn relative_luminance(red: u8, green: u8, blue: u8) -> f64 {
    let red_linear = srgb_channel_to_linear(red);
    let green_linear = srgb_channel_to_linear(green);
    let blue_linear = srgb_channel_to_linear(blue);
    0.2126_f64.mul_add(
        red_linear,
        0.7152_f64.mul_add(green_linear, 0.0722 * blue_linear),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press_char(character: char, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(character), modifiers))
    }

    fn release_char(character: char, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char(character),
            modifiers,
            KeyEventKind::Release,
        ))
    }

    fn repeat_char(character: char, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char(character),
            modifiers,
            KeyEventKind::Repeat,
        ))
    }

    fn press_esc() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
    }

    fn release_esc() -> Event {
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Esc,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    }

    fn ascii_body_events(body: &str, is_uppercase_shifted: bool) -> Vec<Event> {
        body.chars()
            .map(|body_char| {
                let modifiers = if is_uppercase_shifted && body_char.is_ascii_uppercase() {
                    KeyModifiers::SHIFT
                } else {
                    KeyModifiers::NONE
                };
                press_char(body_char, modifiers)
            })
            .collect()
    }

    fn assert_rejects_sequence(parser: &mut ColorReplyParser, mut sequence: Vec<Event>) {
        let expected_rejected = sequence.clone();
        let failing_event = sequence.pop().expect("sequence must be non-empty");
        for buffering_event in sequence {
            assert_eq!(parser.feed(buffering_event), ColorStep::Buffering);
        }
        assert_eq!(
            parser.feed(failing_event),
            ColorStep::Rejected(expected_rejected)
        );
    }

    #[test]
    fn parses_bel_terminated_uppercase_hex_with_shift_modifiers_and_windows_key_releases() {
        let mut parser = ColorReplyParser::new();

        assert_eq!(
            parser.feed(press_char(']', KeyModifiers::ALT)),
            ColorStep::Buffering
        );
        assert_eq!(
            parser.feed(release_char(']', KeyModifiers::ALT)),
            ColorStep::Buffering
        );

        for body_char in "11;rgb:FFFF/FFFF/FFFF".chars() {
            let modifiers = if body_char.is_ascii_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            assert_eq!(
                parser.feed(press_char(body_char, modifiers)),
                ColorStep::Buffering
            );
            assert_eq!(
                parser.feed(release_char(body_char, modifiers)),
                ColorStep::Buffering
            );
        }

        let final_step = parser.feed(press_char('g', KeyModifiers::CONTROL));
        let ColorStep::Complete(reply) = final_step else {
            panic!("expected Complete reply, got {final_step:?}");
        };

        assert_eq!((reply.r, reply.g, reply.b), (255, 255, 255));
        assert!(!reply.is_dark);
        assert!(reply.luminance > DARK_LUMINANCE_THRESHOLD);
        assert_eq!(
            reply.channels,
            [
                String::from("FFFF"),
                String::from("FFFF"),
                String::from("FFFF")
            ]
        );

        let trailing_release = release_char('g', KeyModifiers::CONTROL);
        assert_eq!(
            parser.feed(trailing_release.clone()),
            ColorStep::Idle(trailing_release)
        );
    }

    #[test]
    fn parses_split_esc_prefix_and_st_terminator_and_restores_rejected_events_in_order() {
        let mut parser = ColorReplyParser::new();

        let esc_event = press_esc();
        let k_event = press_char('k', KeyModifiers::NONE);
        assert_eq!(parser.feed(esc_event.clone()), ColorStep::Buffering);
        assert_eq!(parser.feed(release_esc()), ColorStep::Buffering);
        assert_eq!(
            parser.feed(k_event.clone()),
            ColorStep::Rejected(vec![esc_event, k_event])
        );

        let resize_event = Event::Resize(120, 40);
        assert_eq!(
            parser.feed(resize_event.clone()),
            ColorStep::Idle(resize_event)
        );

        assert_eq!(parser.feed(press_esc()), ColorStep::Buffering);
        for event in ascii_body_events("]11;rgb:1a/1b/26", false) {
            assert_eq!(parser.feed(event), ColorStep::Buffering);
        }
        assert_eq!(parser.feed(press_esc()), ColorStep::Buffering);
        let final_step = parser.feed(press_char('\\', KeyModifiers::NONE));
        let ColorStep::Complete(reply) = final_step else {
            panic!("expected Complete reply, got {final_step:?}");
        };

        assert_eq!((reply.r, reply.g, reply.b), (0x1a, 0x1b, 0x26));
        assert!(reply.is_dark);
        assert!(reply.luminance < DARK_LUMINANCE_THRESHOLD);
        assert_eq!(
            reply.channels,
            [String::from("1a"), String::from("1b"), String::from("26")]
        );
    }

    #[test]
    fn scales_one_two_three_and_four_digit_channels_with_alt_backslash_and_raw_bel_terminators() {
        let mut parser = ColorReplyParser::new();

        assert_eq!(
            parser.feed(press_char(']', KeyModifiers::ALT)),
            ColorStep::Buffering
        );
        for event in ascii_body_events("11;rgb:f/0/8", false) {
            assert_eq!(parser.feed(event), ColorStep::Buffering);
        }
        let one_digit_step = parser.feed(press_char('\\', KeyModifiers::ALT));
        let ColorStep::Complete(one_digit_reply) = one_digit_step else {
            panic!("expected Complete for 1-digit channels, got {one_digit_step:?}");
        };
        assert_eq!(
            (one_digit_reply.r, one_digit_reply.g, one_digit_reply.b),
            (255, 0, 136)
        );
        assert_eq!(
            one_digit_reply.channels,
            [String::from("f"), String::from("0"), String::from("8")]
        );

        assert_eq!(
            parser.feed(press_char(']', KeyModifiers::ALT)),
            ColorStep::Buffering
        );
        for event in ascii_body_events("11;rgb:abc/DEF/012", false) {
            assert_eq!(parser.feed(event), ColorStep::Buffering);
        }
        let three_digit_step = parser.feed(press_char('\x07', KeyModifiers::NONE));
        let ColorStep::Complete(three_digit_reply) = three_digit_step else {
            panic!("expected Complete for 3-digit channels, got {three_digit_step:?}");
        };
        assert_eq!(
            (
                three_digit_reply.r,
                three_digit_reply.g,
                three_digit_reply.b
            ),
            (171, 222, 1)
        );
        assert!(!three_digit_reply.is_dark);
    }

    #[test]
    fn rejects_fifth_hex_digit_empty_channels_extra_slash_and_aborted_split_st() {
        let mut parser = ColorReplyParser::new();

        let mut fifth_digit = vec![press_char(']', KeyModifiers::ALT)];
        fifth_digit.extend(ascii_body_events("11;rgb:12345", false));
        assert_rejects_sequence(&mut parser, fifth_digit);

        let mut empty_first_channel = vec![press_char(']', KeyModifiers::ALT)];
        empty_first_channel.extend(ascii_body_events("11;rgb:/", false));
        assert_rejects_sequence(&mut parser, empty_first_channel);

        let mut premature_bel = vec![press_char(']', KeyModifiers::ALT)];
        premature_bel.extend(ascii_body_events("11;rgb:00/00/", false));
        premature_bel.push(press_char('g', KeyModifiers::CONTROL));
        assert_rejects_sequence(&mut parser, premature_bel);

        let mut fourth_channel_slash = vec![press_char(']', KeyModifiers::ALT)];
        fourth_channel_slash.extend(ascii_body_events("11;rgb:00/00/00/", false));
        assert_rejects_sequence(&mut parser, fourth_channel_slash);

        let mut aborted_split_st = vec![press_char(']', KeyModifiers::ALT)];
        aborted_split_st.extend(ascii_body_events("11;rgb:00/00/00", false));
        aborted_split_st.push(press_esc());
        aborted_split_st.push(press_char('x', KeyModifiers::NONE));
        assert_rejects_sequence(&mut parser, aborted_split_st);
    }

    #[test]
    fn rejects_disallowed_modifiers_and_key_repeats_on_prefix_and_hex_digits() {
        let mut parser = ColorReplyParser::new();

        assert_rejects_sequence(
            &mut parser,
            vec![
                press_char(']', KeyModifiers::ALT),
                press_char('1', KeyModifiers::SHIFT),
            ],
        );

        let mut shifted_lowercase_hex = vec![press_char(']', KeyModifiers::ALT)];
        shifted_lowercase_hex.extend(ascii_body_events("11;rgb:", false));
        shifted_lowercase_hex.push(press_char('a', KeyModifiers::SHIFT));
        assert_rejects_sequence(&mut parser, shifted_lowercase_hex);

        let mut alt_uppercase_hex = vec![press_char(']', KeyModifiers::ALT)];
        alt_uppercase_hex.extend(ascii_body_events("11;rgb:", false));
        alt_uppercase_hex.push(press_char('A', KeyModifiers::ALT | KeyModifiers::SHIFT));
        assert_rejects_sequence(&mut parser, alt_uppercase_hex);

        assert_rejects_sequence(
            &mut parser,
            vec![
                press_char(']', KeyModifiers::ALT),
                repeat_char('1', KeyModifiers::NONE),
            ],
        );

        let idle_repeat_esc = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Esc,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        assert_eq!(
            parser.feed(idle_repeat_esc.clone()),
            ColorStep::Idle(idle_repeat_esc)
        );
    }

    #[test]
    fn flushes_incomplete_buffer_on_timeout_and_handles_non_key_interruptions() {
        let mut parser = ColorReplyParser::new();

        assert!(parser.flush_incomplete().is_empty());

        let esc_event = press_esc();
        let bracket_event = press_char(']', KeyModifiers::NONE);
        let one_event = press_char('1', KeyModifiers::NONE);
        assert_eq!(parser.feed(esc_event.clone()), ColorStep::Buffering);
        assert_eq!(parser.feed(release_esc()), ColorStep::Buffering);
        assert_eq!(parser.feed(bracket_event.clone()), ColorStep::Buffering);
        assert_eq!(parser.feed(one_event.clone()), ColorStep::Buffering);

        assert_eq!(
            parser.flush_incomplete(),
            vec![esc_event, bracket_event, one_event]
        );
        assert!(parser.flush_incomplete().is_empty());

        assert_rejects_sequence(
            &mut parser,
            vec![press_char(']', KeyModifiers::ALT), Event::FocusLost],
        );
    }

    #[test]
    fn computes_wcag_relative_luminance_boundaries_and_threshold() {
        assert_eq!(INITIAL_COLOR_WAIT_MS, 200);
        assert_eq!(ARMED_COLOR_WINDOW_MS, 2_000);
        assert!((DARK_LUMINANCE_THRESHOLD - 0.179).abs() < 1e-12);

        let black_luminance = relative_luminance(0, 0, 0);
        let white_luminance = relative_luminance(255, 255, 255);
        assert!(black_luminance.abs() < 1e-12);
        assert!((white_luminance - 1.0).abs() < 1e-12);

        assert!((relative_luminance(255, 0, 0) - 0.2126).abs() < 1e-12);
        assert!((relative_luminance(0, 255, 0) - 0.7152).abs() < 1e-12);
        assert!((relative_luminance(0, 0, 255) - 0.0722).abs() < 1e-12);

        assert!(relative_luminance(0x76, 0x76, 0x76) > DARK_LUMINANCE_THRESHOLD);
        assert!(relative_luminance(0x75, 0x75, 0x75) < DARK_LUMINANCE_THRESHOLD);
    }
}
