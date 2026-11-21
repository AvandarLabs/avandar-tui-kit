use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{ColorReply, ColorReplyParser, ColorStep, DARK_LUMINANCE_THRESHOLD};

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

fn feed_with_windows_releases(parser: &mut ColorReplyParser, character: char) {
    let modifiers = if character.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    assert_eq!(
        parser.feed(press_char(character, modifiers)),
        ColorStep::Buffering
    );
    assert_eq!(
        parser.feed(release_char(character, modifiers)),
        ColorStep::Buffering
    );
}

fn parse_alt_osc11_reply(
    parser: &mut ColorReplyParser,
    rgb_channels: &str,
    terminator: Event,
) -> ColorReply {
    assert_eq!(
        parser.feed(press_char(']', KeyModifiers::ALT)),
        ColorStep::Buffering
    );
    for event in ascii_body_events(&format!("11;rgb:{rgb_channels}"), false) {
        assert_eq!(parser.feed(event), ColorStep::Buffering);
    }
    let final_step = parser.feed(terminator);
    let ColorStep::Complete(reply) = final_step else {
        panic!("expected Complete reply for {rgb_channels}, got {final_step:?}");
    };
    reply
}

fn alt_osc11_sequence(body: &str) -> Vec<Event> {
    let mut sequence = vec![press_char(']', KeyModifiers::ALT)];
    sequence.extend(ascii_body_events(body, false));
    sequence
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
fn bel_terminated_uppercase_reply_decodes_white_channels_despite_windows_releases() {
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
        feed_with_windows_releases(&mut parser, body_char);
    }

    let final_step = parser.feed(press_char('g', KeyModifiers::CONTROL));
    let ColorStep::Complete(reply) = final_step else {
        panic!("expected Complete reply, got {final_step:?}");
    };
    assert_eq!((reply.r, reply.g, reply.b), (255, 255, 255));
    assert!(!reply.is_dark);
    assert!(reply.luminance > DARK_LUMINANCE_THRESHOLD);
    assert_eq!(reply.channels, ["FFFF", "FFFF", "FFFF"].map(String::from));

    let trailing_release = release_char('g', KeyModifiers::CONTROL);
    assert_eq!(
        parser.feed(trailing_release.clone()),
        ColorStep::Idle(trailing_release)
    );
}

#[test]
fn rejected_escape_prefix_replays_buffered_events_in_arrival_order() {
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
}

#[test]
fn split_escape_prefix_and_st_terminator_decode_dark_rgb_reply() {
    let mut parser = ColorReplyParser::new();

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
    assert_eq!(reply.channels, ["1a", "1b", "26"].map(String::from));
}

#[test]
fn one_to_four_digit_hex_channels_scale_proportionally_to_eight_bits() {
    let mut parser = ColorReplyParser::new();

    let one_digit =
        parse_alt_osc11_reply(&mut parser, "f/0/8", press_char('\\', KeyModifiers::ALT));
    assert_eq!((one_digit.r, one_digit.g, one_digit.b), (255, 0, 136));
    assert_eq!(one_digit.channels, ["f", "0", "8"].map(String::from));

    let two_digit = parse_alt_osc11_reply(
        &mut parser,
        "10/80/ff",
        press_char('\x07', KeyModifiers::NONE),
    );
    assert_eq!((two_digit.r, two_digit.g, two_digit.b), (0x10, 0x80, 0xff));

    let three_digit = parse_alt_osc11_reply(
        &mut parser,
        "abc/DEF/012",
        press_char('\x07', KeyModifiers::NONE),
    );
    assert_eq!((three_digit.r, three_digit.g, three_digit.b), (171, 222, 1));
    assert!(!three_digit.is_dark);

    let four_digit = parse_alt_osc11_reply(
        &mut parser,
        "0000/8080/ffff",
        press_char('\\', KeyModifiers::ALT),
    );
    assert_eq!(
        (four_digit.r, four_digit.g, four_digit.b),
        (0x00, 0x80, 0xff)
    );
}

#[test]
fn malformed_channel_lengths_and_terminators_replay_buffered_events() {
    let mut parser = ColorReplyParser::new();

    assert_rejects_sequence(&mut parser, alt_osc11_sequence("11;rgb:12345"));
    assert_rejects_sequence(&mut parser, alt_osc11_sequence("11;rgb:/"));

    let mut premature_bel = alt_osc11_sequence("11;rgb:00/00/");
    premature_bel.push(press_char('g', KeyModifiers::CONTROL));
    assert_rejects_sequence(&mut parser, premature_bel);

    assert_rejects_sequence(&mut parser, alt_osc11_sequence("11;rgb:00/00/00/"));

    let mut aborted_split_st = alt_osc11_sequence("11;rgb:00/00/00");
    aborted_split_st.push(press_esc());
    aborted_split_st.push(press_char('x', KeyModifiers::NONE));
    assert_rejects_sequence(&mut parser, aborted_split_st);
}

#[test]
fn disallowed_modifiers_and_key_repeats_reject_in_progress_reply() {
    let mut parser = ColorReplyParser::new();

    assert_rejects_sequence(
        &mut parser,
        vec![
            press_char(']', KeyModifiers::ALT),
            press_char('1', KeyModifiers::SHIFT),
        ],
    );

    let mut shifted_lowercase_hex = alt_osc11_sequence("11;rgb:");
    shifted_lowercase_hex.push(press_char('a', KeyModifiers::SHIFT));
    assert_rejects_sequence(&mut parser, shifted_lowercase_hex);

    let mut alt_uppercase_hex = alt_osc11_sequence("11;rgb:");
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
fn timeout_flush_drains_buffered_prefix_events_once() {
    let mut parser = ColorReplyParser::new();

    assert_eq!(parser.flush_incomplete(), Vec::<Event>::new());

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
    assert_eq!(parser.flush_incomplete(), Vec::<Event>::new());
}

#[test]
fn non_key_event_during_buffering_replays_prefix_with_interrupting_event() {
    let mut parser = ColorReplyParser::new();
    assert_rejects_sequence(
        &mut parser,
        vec![press_char(']', KeyModifiers::ALT), Event::FocusLost],
    );
}
