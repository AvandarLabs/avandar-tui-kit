//! Real emulator history, cell, and viewport surface contracts.

#![cfg(feature = "vt100")]

pub mod common;

use common::surface_contract::{
    ROWS, VISIBLE_ROWS, WIDTH, WRAPPED_ROWS, assert_surface_contract, point,
};
use ratatui_mouse::{ContentRange, SelectableSurface, Vt100Surface, WordCharacters};

#[test]
fn history_wraps_and_offset_restoration() {
    let mut parser = vt100::Parser::new(3, 8, 20);
    parser.process(b"r0\r\nr1\r\nr2\r\nr3\r\nabcdefghijklmnopqrs\r\nend");
    parser.screen_mut().set_scrollback(2);
    let saved_offset = parser.screen().scrollback();
    {
        let mut surface = Vt100Surface::new(parser.screen_mut());
        let saved_origin = surface.origin();
        assert_eq!(
            surface.text(ContentRange::new(point(0, 0), point(7, 7))),
            "r0\nr1\nr2\nr3\nabcdefghijklmnopqrs\nend"
        );
        assert_eq!(surface.line_bounds(), 0..=7);
        assert_eq!(saved_origin, point(3, 0));
        for line in 4..=6 {
            let range = surface.line_at(line);
            assert_eq!(range.start, point(4, 0));
            assert_eq!(range.end.line, 6);
            assert_eq!(surface.text(range), "abcdefghijklmnopqrs");
        }
        assert_eq!(surface.origin(), saved_origin);
    }
    assert_eq!(parser.screen().scrollback(), saved_offset);
}

#[test]
fn line_numbers_follow_oldest_history_until_capacity() {
    let mut parser = vt100::Parser::new(3, 8, 4);
    parser.process(b"zero\r\none\r\ntwo\r\nthree");
    let range = ContentRange::new(point(1, 0), point(1, 7));
    for output in ["", "\r\nfour", "\r\nfive", "\r\nsix"] {
        parser.process(output.as_bytes());
        parser.screen_mut().set_scrollback(1);
        let mut surface = Vt100Surface::new(parser.screen_mut());
        let saved_origin = surface.origin();
        assert_eq!(surface.text(range), "one");
        assert_eq!(surface.origin(), saved_origin);
    }
    parser.process(b"\r\nseven");
    let mut surface = Vt100Surface::new(parser.screen_mut());
    assert_eq!(surface.line_bounds(), 0..=6);
    assert_eq!(surface.text(range), "two");
}

#[test]
fn alternate_screen_has_no_history_and_cannot_scroll() {
    let mut parser = vt100::Parser::new(3, 8, 20);
    parser.process(b"zero\r\none\r\ntwo\r\nthree\r\nfour");
    parser.screen_mut().set_scrollback(2);
    parser.process(b"\x1b[?1049halt");
    {
        let mut surface = Vt100Surface::new(parser.screen_mut());
        assert_eq!(surface.origin(), point(0, 0));
        assert_eq!(surface.line_bounds(), 0..=2);
        assert_eq!(surface.scroll_by(i32::MIN), 0);
        assert_eq!(surface.scroll_by(i32::MAX), 0);
        assert_eq!(
            surface.text(ContentRange::new(point(0, 0), point(2, 7))),
            "alt"
        );
    }
    assert_eq!(parser.screen().scrollback(), 0);
}

#[test]
fn wide_cells_and_combining_text_are_copied_whole() {
    let mut parser = vt100::Parser::new(2, 8, 20);
    parser.process("a界b\r\ne\u{301}  x".as_bytes());
    let mut surface = Vt100Surface::new(parser.screen_mut());
    for column in [1, 2] {
        let position = point(0, column);
        assert_eq!(
            surface.character_at(position),
            ContentRange::new(point(0, 1), point(0, 2))
        );
        assert_eq!(surface.text(ContentRange::new(position, position)), "界");
        assert_eq!(
            surface.word_at(position, &WordCharacters::default()),
            Some(ContentRange::new(point(0, 0), point(0, 3)))
        );
    }
    assert_eq!(
        surface.text(ContentRange::new(point(1, 0), point(1, 3))),
        "e\u{301}  x"
    );
}

#[test]
fn vt100_surface_obeys_shared_contract() {
    let mut parser = vt100::Parser::new(VISIBLE_ROWS, WIDTH, 20);
    for (index, row) in ROWS.iter().enumerate() {
        parser.process(row.as_bytes());
        if index + 1 < ROWS.len() && !WRAPPED_ROWS[index] {
            parser.process(b"\r\n");
        }
    }
    parser.screen_mut().set_scrollback(1);
    assert_surface_contract(&mut Vt100Surface::new(parser.screen_mut()));
}

#[test]
fn missing_rows_and_columns_are_safe_and_preserve_origin() {
    let mut parser = vt100::Parser::new(2, 8, 20);
    parser.process(b"one\r\ntwo\r\nthree");
    parser.screen_mut().set_scrollback(1);
    let mut surface = Vt100Surface::new(parser.screen_mut());
    let saved_origin = surface.origin();
    for position in [point(-1, 0), point(i64::MAX, 0), point(0, u32::MAX)] {
        assert_eq!(
            surface.character_at(position),
            ContentRange::new(position, position)
        );
        assert_eq!(surface.word_at(position, &WordCharacters::default()), None);
        assert_eq!(surface.text(ContentRange::new(position, position)), "");
    }
    assert_eq!(
        surface.line_at(-1),
        ContentRange::new(point(-1, 0), point(-1, 0))
    );
    assert_eq!(surface.origin(), saved_origin);
}
