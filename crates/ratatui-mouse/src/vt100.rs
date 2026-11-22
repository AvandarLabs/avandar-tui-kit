//! Borrowed selection over an emulator's cells and retained history.

use std::ops::RangeInclusive;

use crate::{
    ContentPoint, ContentRange, MouseEncoding, MouseTracking, PaneMouseRequest, SelectableSurface,
    WordCharacters,
};

impl PaneMouseRequest {
    /// What the program writing to `screen` asked for.
    #[must_use]
    pub fn from_screen(screen: &::vt100::Screen) -> Self {
        Self {
            tracking: match screen.mouse_protocol_mode() {
                ::vt100::MouseProtocolMode::None => MouseTracking::Off,
                ::vt100::MouseProtocolMode::Press => MouseTracking::Press,
                ::vt100::MouseProtocolMode::PressRelease => MouseTracking::PressRelease,
                ::vt100::MouseProtocolMode::ButtonMotion => MouseTracking::ButtonMotion,
                ::vt100::MouseProtocolMode::AnyMotion => MouseTracking::AnyMotion,
            },
            encoding: match screen.mouse_protocol_encoding() {
                ::vt100::MouseProtocolEncoding::Default => MouseEncoding::Default,
                ::vt100::MouseProtocolEncoding::Utf8 => MouseEncoding::Utf8,
                ::vt100::MouseProtocolEncoding::Sgr => MouseEncoding::Sgr,
            },
            is_alternate_screen: screen.alternate_screen(),
            is_application_cursor: screen.application_cursor(),
        }
    }
}

/// A surface over a terminal emulator's screen and its scrollback.
///
/// Line zero is the oldest retained history row, followed by the live screen.
/// Coordinates remain stable as output arrives until history reaches capacity;
/// each subsequent evicted row shifts retained content one line toward zero.
/// The mutable borrow prevents output or resizing while this adapter exists.
pub struct Vt100Surface<'a> {
    screen: &'a mut ::vt100::Screen,
    history_rows: usize,
    visible_rows: u16,
    columns: u16,
}

impl<'a> Vt100Surface<'a> {
    /// Borrow `screen`, numbering content from its oldest retained history row.
    /// Discovering history preserves the current viewport offset. Alternate
    /// screens have no history. Reconstruct after output to refresh the bounds.
    #[must_use]
    pub fn new(screen: &'a mut ::vt100::Screen) -> Self {
        let history_rows = {
            let guard = ScrollbackGuard::new(screen);
            guard.screen.set_scrollback(usize::MAX);
            guard.screen.scrollback()
        };
        let (visible_rows, columns) = screen.size();
        Self {
            screen,
            history_rows,
            visible_rows,
            columns,
        }
    }

    fn row(&mut self, line: i64) -> Option<SurfaceRow> {
        let line = usize::try_from(line).ok()?;
        if line
            >= self
                .history_rows
                .saturating_add(usize::from(self.visible_rows))
        {
            return None;
        }
        let guard = ScrollbackGuard::new(self.screen);
        guard
            .screen
            .set_scrollback(self.history_rows.saturating_sub(line));
        let row = u16::try_from(line.saturating_sub(self.history_rows)).ok()?;
        Some(SurfaceRow {
            cells: (0..self.columns)
                .filter_map(|column| guard.screen.cell(row, column).cloned())
                .collect(),
            is_wrapped: guard.screen.row_wrapped(row)
                && line + 1
                    < self
                        .history_rows
                        .saturating_add(usize::from(self.visible_rows)),
        })
    }
}

impl SelectableSurface for Vt100Surface<'_> {
    fn origin(&self) -> ContentPoint {
        ContentPoint {
            line: i64::try_from(self.history_rows.saturating_sub(self.screen.scrollback()))
                .unwrap_or(i64::MAX),
            column: 0,
        }
    }

    fn line_bounds(&self) -> RangeInclusive<i64> {
        0..=i64::try_from(
            self.history_rows
                .saturating_add(usize::from(self.visible_rows)),
        )
        .unwrap_or(i64::MAX)
        .saturating_sub(1)
    }

    fn character_at(&mut self, point: ContentPoint) -> ContentRange {
        let (first, last) = self
            .row(point.line)
            .map_or((point.column, point.column), |row| {
                row.character_columns(point.column)
            });
        range_in_row(point.line, first, last)
    }

    fn word_at(
        &mut self,
        point: ContentPoint,
        word_characters: &WordCharacters,
    ) -> Option<ContentRange> {
        let row = self.row(point.line)?;
        let (mut first, mut last) = row.character_columns(point.column);
        if !row.is_word(first, word_characters) {
            return None;
        }
        while first > 0 && row.is_word(first - 1, word_characters) {
            first -= 1;
        }
        while last < u32::from(self.columns.saturating_sub(1))
            && row.is_word(last + 1, word_characters)
        {
            last += 1;
        }
        Some(range_in_row(point.line, first, last))
    }

    fn line_at(&mut self, line: i64) -> ContentRange {
        if !self.line_bounds().contains(&line) {
            return range_in_row(line, 0, 0);
        }
        let mut first = line;
        let mut last = line;
        while first > 0 && self.row(first - 1).is_some_and(|row| row.is_wrapped) {
            first -= 1;
        }
        while self.row(last).is_some_and(|row| row.is_wrapped) {
            last += 1;
        }
        ContentRange::new(
            ContentPoint {
                line: first,
                column: 0,
            },
            ContentPoint {
                line: last,
                column: u32::from(self.columns.saturating_sub(1)),
            },
        )
    }

    fn text(&mut self, range: ContentRange) -> String {
        let mut text = String::new();
        let first = range.start.line.max(0);
        let last = range.end.line.min(*self.line_bounds().end());
        for line in first..=last {
            let Some(row) = self.row(line) else { continue };
            let first_column = if line == range.start.line {
                range.start.column
            } else {
                0
            };
            let last_column = if line == range.end.line {
                range.end.column
            } else {
                u32::MAX
            };
            row.append_text(&mut text, first_column, last_column);
            if line == last || !row.is_wrapped {
                text.truncate(
                    text.trim_end_matches(|character: char| {
                        character != '\n' && character.is_whitespace()
                    })
                    .len(),
                );
                if line != last {
                    text.push('\n');
                }
            }
        }
        text.truncate(text.trim_end_matches('\n').len());
        text
    }

    fn scroll_by(&mut self, rows: i32) -> i32 {
        let previous = self.screen.scrollback();
        let requested = i128::try_from(previous).unwrap_or(i128::MAX) - i128::from(rows);
        self.screen
            .set_scrollback(usize::try_from(requested.max(0)).unwrap_or(usize::MAX));
        i32::try_from(
            i128::try_from(previous).unwrap_or(i128::MAX)
                - i128::try_from(self.screen.scrollback()).unwrap_or(i128::MAX),
        )
        .unwrap_or(0)
    }

    fn pane_request(&self) -> Option<PaneMouseRequest> {
        Some(PaneMouseRequest::from_screen(self.screen))
    }
}

struct SurfaceRow {
    cells: Vec<::vt100::Cell>,
    is_wrapped: bool,
}

impl SurfaceRow {
    fn character_columns(&self, column: u32) -> (u32, u32) {
        let Some(cell) = usize::try_from(column)
            .ok()
            .and_then(|index| self.cells.get(index))
        else {
            return (column, column);
        };
        if cell.is_wide_continuation() {
            (column.saturating_sub(1), column)
        } else if cell.is_wide() {
            (column, column.saturating_add(1))
        } else {
            (column, column)
        }
    }

    fn is_word(&self, column: u32, word_characters: &WordCharacters) -> bool {
        let (first, _) = self.character_columns(column);
        usize::try_from(first)
            .ok()
            .and_then(|index| self.cells.get(index))
            .and_then(|cell| cell.contents().chars().next())
            .is_some_and(|character| {
                !character.is_whitespace() && word_characters.contains(character)
            })
    }

    fn append_text(&self, text: &mut String, first: u32, last: u32) {
        for (index, cell) in self.cells.iter().enumerate() {
            let column = u32::try_from(index).unwrap_or(u32::MAX);
            let (_, character_end) = self.character_columns(column);
            if cell.is_wide_continuation() || column > last || character_end < first {
                continue;
            }
            if cell.has_contents() {
                text.push_str(cell.contents());
            } else {
                text.push(' ');
            }
        }
    }
}

fn range_in_row(line: i64, first: u32, last: u32) -> ContentRange {
    ContentRange::new(
        ContentPoint {
            line,
            column: first,
        },
        ContentPoint { line, column: last },
    )
}

// Every temporary viewport change is restored, including during unwinding.
struct ScrollbackGuard<'a> {
    screen: &'a mut ::vt100::Screen,
    saved_offset: usize,
}

impl<'a> ScrollbackGuard<'a> {
    fn new(screen: &'a mut ::vt100::Screen) -> Self {
        let saved_offset = screen.scrollback();
        Self {
            screen,
            saved_offset,
        }
    }
}

impl Drop for ScrollbackGuard<'_> {
    fn drop(&mut self) {
        self.screen.set_scrollback(self.saved_offset);
    }
}
