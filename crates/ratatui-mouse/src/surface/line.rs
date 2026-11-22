use std::ops::RangeInclusive;

use crate::words::line_cell_width;
use crate::{
    ContentPoint, ContentRange, ScrollPosition, SelectableSurface, WordCharacters,
    character_columns_in_line, text_in_columns, word_columns_in_line,
};

/// A surface over rows of text the app rendered itself.
pub struct LineSurface<'a, L: AsRef<str>> {
    rows: &'a [L],
    scroll: &'a mut ScrollPosition,
    visible_rows: u16,
    is_row_wrapped: &'a [bool],
}

impl<'a, L: AsRef<str>> LineSurface<'a, L> {
    /// Borrow visual rows and their app-owned viewport scroll position.
    /// An out-of-bounds starting row is clamped to the last full viewport.
    /// A zero-height viewport uses one row when calculating scroll limits.
    #[must_use]
    pub fn new(rows: &'a [L], scroll: &'a mut ScrollPosition, visible_rows: u16) -> Self {
        scroll.line = scroll
            .line
            .min(rows.len().saturating_sub(usize::from(visible_rows.max(1))));
        Self {
            rows,
            scroll,
            visible_rows,
            is_row_wrapped: &[],
        }
    }

    /// Identify rows that soft-wrap into the following row.
    /// Missing entries are treated as unwrapped; excess entries are ignored.
    #[must_use]
    pub const fn with_wrapped_rows(mut self, is_row_wrapped: &'a [bool]) -> Self {
        self.is_row_wrapped = is_row_wrapped;
        self
    }

    fn row(&self, line: i64) -> Option<&str> {
        self.rows
            .get(usize::try_from(line).ok()?)
            .map(AsRef::as_ref)
    }

    fn text_in_row(&self, row: usize, range: ContentRange) -> &str {
        let line = i64::try_from(row).unwrap_or(i64::MAX);
        let first = if line == range.start.line {
            range.start.column
        } else {
            0
        };
        let last = if line == range.end.line {
            range.end.column
        } else {
            u32::MAX
        };
        text_in_columns(self.rows[row].as_ref(), first, last)
    }

    fn is_wrapped(&self, row: usize) -> bool {
        row < self.rows.len().saturating_sub(1)
            && self.is_row_wrapped.get(row).copied().unwrap_or(false)
    }
}

impl<L: AsRef<str>> SelectableSurface for LineSurface<'_, L> {
    fn origin(&self) -> ContentPoint {
        ContentPoint {
            line: i64::try_from(self.scroll.line).unwrap_or(i64::MAX),
            column: self.scroll.column,
        }
    }

    fn line_bounds(&self) -> RangeInclusive<i64> {
        0..=i64::try_from(self.rows.len())
            .unwrap_or(i64::MAX)
            .saturating_sub(1)
    }

    fn character_at(&mut self, point: ContentPoint) -> ContentRange {
        let (first, last) = self
            .row(point.line)
            .and_then(|row| character_columns_in_line(row, point.column))
            .unwrap_or((point.column, point.column));
        ContentRange::new(
            ContentPoint {
                column: first,
                ..point
            },
            ContentPoint {
                column: last,
                ..point
            },
        )
    }

    fn word_at(
        &mut self,
        point: ContentPoint,
        word_characters: &WordCharacters,
    ) -> Option<ContentRange> {
        let (first, last) =
            word_columns_in_line(self.row(point.line)?, point.column, word_characters)?;
        Some(ContentRange::new(
            ContentPoint {
                column: first,
                ..point
            },
            ContentPoint {
                column: last,
                ..point
            },
        ))
    }

    fn line_at(&mut self, line: i64) -> ContentRange {
        let point = ContentPoint { line, column: 0 };
        let Some(mut first) = usize::try_from(line)
            .ok()
            .filter(|&row| row < self.rows.len())
        else {
            return ContentRange::new(point, point);
        };
        let mut last = first;
        while first > 0 && self.is_wrapped(first - 1) {
            first -= 1;
        }
        while self.is_wrapped(last) {
            last += 1;
        }
        ContentRange::new(
            ContentPoint {
                line: i64::try_from(first).unwrap_or(i64::MAX),
                column: 0,
            },
            ContentPoint {
                line: i64::try_from(last).unwrap_or(i64::MAX),
                column: line_cell_width(self.rows[last].as_ref()).saturating_sub(1),
            },
        )
    }

    fn text(&mut self, range: ContentRange) -> String {
        let first = usize::try_from(range.start.line.max(0)).unwrap_or(usize::MAX);
        let Some(last) = usize::try_from(range.end.line)
            .ok()
            .map(|line| line.min(self.rows.len().saturating_sub(1)))
        else {
            return String::new();
        };
        let mut text = String::new();
        for (row_index, _) in self
            .rows
            .iter()
            .enumerate()
            .take(last.saturating_add(1))
            .skip(first)
        {
            text.push_str(self.text_in_row(row_index, range));
            if row_index == last || !self.is_wrapped(row_index) {
                text.truncate(
                    text.trim_end_matches(|character: char| {
                        character != '\n' && character.is_whitespace()
                    })
                    .len(),
                );
                if row_index != last {
                    text.push('\n');
                }
            }
        }
        text.truncate(text.trim_end_matches('\n').len());
        text
    }

    fn scroll_by(&mut self, rows: i32) -> i32 {
        let previous = self.scroll.line;
        let maximum = self
            .rows
            .len()
            .saturating_sub(usize::from(self.visible_rows.max(1)));
        let requested = i128::try_from(previous).unwrap_or(i128::MAX) + i128::from(rows);
        self.scroll.line = usize::try_from(requested.max(0))
            .unwrap_or(usize::MAX)
            .min(maximum);
        i32::try_from(
            i128::try_from(self.scroll.line).unwrap_or(i128::MAX)
                - i128::try_from(previous).unwrap_or(i128::MAX),
        )
        .unwrap_or(0)
    }
}
