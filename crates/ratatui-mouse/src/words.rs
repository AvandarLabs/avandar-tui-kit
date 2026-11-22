//! Cell-aware lookup over original grapheme clusters.

use ratatui::buffer::CellWidth;
use unicode_segmentation::UnicodeSegmentation;

use crate::WordCharacters;

/// The first and last column, in cells, of the word at `column`.
/// Returns `None` past the end, on blanks, or on non-word characters.
#[must_use]
pub fn word_columns_in_line(
    line: &str,
    column: u32,
    word_characters: &WordCharacters,
) -> Option<(u32, u32)> {
    let mut word_start = None;
    let mut word_end = 0;
    for (_, grapheme, first, last) in grapheme_cells(line) {
        let is_word = grapheme.chars().next().is_some_and(|character| {
            !character.is_whitespace() && word_characters.contains(character)
        });
        if is_word {
            word_start.get_or_insert(first);
            word_end = last;
        } else if let Some(start) = word_start.take()
            && (start..=word_end).contains(&column)
        {
            return Some((start, word_end));
        }
    }
    word_start
        .filter(|&start| (start..=word_end).contains(&column))
        .map(|start| (start, word_end))
}

/// The first and last column, in cells, of the character at `column`.
/// Returns `None` past the end of the line. Wide graphemes stay whole.
#[must_use]
pub fn character_columns_in_line(line: &str, column: u32) -> Option<(u32, u32)> {
    grapheme_cells(line)
        .find(|&(_, _, first, last)| (first..=last).contains(&column))
        .map(|(_, _, first, last)| (first, last))
}

/// The part of `line` between two inclusive cell columns.
/// Every grapheme overlapping the range is kept byte-for-byte whole.
/// Reversed ranges and ranges past the end return an empty string.
#[must_use]
pub fn text_in_columns(line: &str, first_column: u32, last_column: u32) -> &str {
    if first_column > last_column {
        return "";
    }
    let mut first_byte = None;
    let mut end_byte = 0;
    for (byte, grapheme, first, last) in grapheme_cells(line) {
        if first > last_column {
            break;
        }
        if last >= first_column {
            first_byte.get_or_insert(byte);
            end_byte = byte + grapheme.len();
        }
    }
    first_byte.map_or("", |start| &line[start..end_byte])
}

pub(crate) fn line_cell_width(line: &str) -> u32 {
    grapheme_cells(line)
        .last()
        .map_or(0, |(_, _, _, last)| last.saturating_add(1))
}

fn grapheme_cells(line: &str) -> impl Iterator<Item = (usize, &str, u32, u32)> {
    let mut column = 0_u32;
    line.grapheme_indices(true)
        .filter_map(move |(byte, grapheme)| {
            if grapheme.contains(char::is_control) {
                return None;
            }
            let width = u32::from(grapheme.cell_width());
            if width == 0 {
                return None;
            }
            let first = column;
            column = column.saturating_add(width);
            Some((byte, grapheme, first, column.saturating_sub(1)))
        })
}
