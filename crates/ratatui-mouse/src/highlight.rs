//! Screen rectangles for an inclusive content selection.

use std::ops::Range;

use crate::{ContentPoint, ContentRange};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
};

/// Reversed video, suitable for selections without an app theme.
pub const DEFAULT_HIGHLIGHT_STYLE: Style = Style::new().add_modifier(Modifier::REVERSED);

/// A selection as at most three screen rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRects {
    rectangles: [Rect; 3],
    length: usize,
}

impl HighlightRects {
    /// The rectangles covering `range` inside `area` for the given surface origin.
    /// Empty when none of the range is visible. Snap range endpoints to whole
    /// characters before calling: this conversion does not inspect a surface.
    #[must_use]
    pub fn from_range(range: ContentRange, origin: ContentPoint, area: Rect) -> Self {
        let mut highlight = Self {
            rectangles: [Rect::default(); 3],
            length: 0,
        };
        let range = ContentRange::new(range.start, range.end);
        let first_line = i128::from(range.start.line);
        let last_line = i128::from(range.end.line);
        let first_column = i128::from(range.start.column);
        let end_column = i128::from(range.end.column) + 1;
        if first_line == last_line {
            highlight.push_clipped(
                first_line..last_line + 1,
                first_column..end_column,
                origin,
                area,
            );
        } else {
            let column_limit = i128::from(u32::MAX) + 1;
            highlight.push_clipped(
                first_line..first_line + 1,
                first_column..column_limit,
                origin,
                area,
            );
            highlight.push_clipped(first_line + 1..last_line, 0..column_limit, origin, area);
            highlight.push_clipped(last_line..last_line + 1, 0..end_column, origin, area);
        }
        highlight
    }

    fn push_clipped(
        &mut self,
        lines: Range<i128>,
        columns: Range<i128>,
        origin: ContentPoint,
        area: Rect,
    ) {
        if let Some(rectangle) = clip_rectangle(lines, columns, origin, area) {
            self.rectangles[self.length] = rectangle;
            self.length += 1;
        }
    }

    /// The rectangles, from top to bottom.
    #[must_use]
    pub fn as_slice(&self) -> &[Rect] {
        &self.rectangles[..self.length]
    }

    /// Patches `style` onto the rectangles' cells inside `buffer`.
    pub fn render(&self, buffer: &mut Buffer, style: Style) {
        for rectangle in self.as_slice() {
            buffer.set_style(rectangle.intersection(buffer.area), style);
        }
    }
}

fn clip_rectangle(
    lines: Range<i128>,
    columns: Range<i128>,
    origin: ContentPoint,
    area: Rect,
) -> Option<Rect> {
    let origin_line = i128::from(origin.line);
    let origin_column = i128::from(origin.column);
    let first_line = lines.start.max(origin_line);
    let end_line = lines
        .end
        .min(origin_line + i128::from(area.bottom() - area.y));
    let first_column = columns.start.max(origin_column);
    let end_column = columns
        .end
        .min(origin_column + i128::from(area.right() - area.x));
    if first_line >= end_line || first_column >= end_column {
        return None;
    }
    Some(Rect::new(
        u16::try_from(i128::from(area.x) + first_column - origin_column).ok()?,
        u16::try_from(i128::from(area.y) + first_line - origin_line).ok()?,
        u16::try_from(end_column - first_column).ok()?,
        u16::try_from(end_line - first_line).ok()?,
    ))
}
