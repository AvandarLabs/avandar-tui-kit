//! Cell-coordinate text selection regressions.

use ratatui_mouse::{
    WordCharacters, character_columns_in_line, text_in_columns, word_columns_in_line,
};

#[test]
fn words_use_cells_and_keep_original_graphemes() {
    for column in [1, 2] {
        assert_eq!(character_columns_in_line("a界b", column), Some((1, 2)));
        assert_eq!(text_in_columns("a界b", column, column), "界");
    }
    assert_eq!(
        word_columns_in_line("file.rs:12", 4, &WordCharacters::default()),
        Some((0, 9))
    );
    assert_eq!(text_in_columns("e\u{301}界", 0, 0), "e\u{301}");
    assert_eq!(character_columns_in_line("e\u{301}界", 1), Some((1, 2)));
    assert_eq!(
        word_columns_in_line("e\u{301}界", 2, &WordCharacters::default()),
        Some((0, 2))
    );
}

#[test]
fn word_boundaries_exclude_blanks_quotes_and_brackets() {
    let characters = WordCharacters::default();
    for column in [0, 1, 5, 6, 7, 9, 10, u32::MAX] {
        assert_eq!(
            word_columns_in_line(" [abc] \"x\"", column, &characters),
            None
        );
    }
    assert_eq!(
        word_columns_in_line(" [abc] \"x\"", 3, &characters),
        Some((2, 4))
    );
    assert_eq!(word_columns_in_line("'x'", 1, &characters), Some((1, 1)));
    assert_eq!(
        word_columns_in_line("/tmp/a.rs", 3, &characters),
        Some((0, 8))
    );
    assert_eq!(
        word_columns_in_line("https://a.b/x?q=1", 8, &characters),
        Some((0, 16))
    );
    assert_eq!(
        word_columns_in_line("a-b", 1, &WordCharacters::new("")),
        None
    );
}

#[test]
fn text_ranges_keep_clusters_and_ignore_missing_cells() {
    assert_eq!(text_in_columns("a界b", 2, 3), "界b");
    assert_eq!(text_in_columns("a界b", 3, u32::MAX), "b");
    assert_eq!(text_in_columns("a界b", 4, 6), "");
    assert_eq!(text_in_columns("a界b", 3, 1), "");
    assert_eq!(character_columns_in_line("", 0), None);
    assert_eq!(character_columns_in_line("abc", 3), None);
    assert_eq!(character_columns_in_line(" ", 0), Some((0, 0)));
    assert_eq!(text_in_columns("👩‍💻!", 1, 1), "👩‍💻");
}

#[test]
fn cell_width_matches_ratatui_halfwidth_sound_marks() {
    let mut buffer = ratatui::buffer::Buffer::empty(ratatui::layout::Rect::new(0, 0, 8, 1));
    buffer.set_string(0, 0, "aｶﾞb", ratatui::style::Style::default());
    assert_eq!(buffer[(4, 0)].symbol(), "b");
    assert_eq!(character_columns_in_line("aｶﾞb", 2), Some((1, 3)));
    assert_eq!(text_in_columns("aｶﾞb", 2, 2), "ｶﾞ");
    assert_eq!(character_columns_in_line("a\nb", 1), Some((1, 1)));
    assert_eq!(character_columns_in_line("\u{301}", 0), None);
}
