//! Content coordinates independent of viewport scrolling.

use ratatui::layout::{Position, Rect};

/// A cell in a surface's content, stable while the surface scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentPoint {
    /// The content line, numbered by the surface.
    pub line: i64,
    /// The column in cells from the start of the content row.
    pub column: u32,
}

impl ContentPoint {
    /// The content point under `position`, which must be inside `area`.
    /// `origin` is the surface's origin. Content coordinates saturate at their
    /// numeric limits if the translated point cannot be represented.
    #[must_use]
    pub fn from_screen_position(position: Position, area: Rect, origin: Self) -> Self {
        Self {
            line: origin
                .line
                .saturating_add(i64::from(position.y.saturating_sub(area.y))),
            column: origin
                .column
                .saturating_add(u32::from(position.x.saturating_sub(area.x))),
        }
    }

    /// Where this point is on screen, or `None` when it is outside `area`.
    #[must_use]
    pub fn to_screen_position(self, area: Rect, origin: Self) -> Option<Position> {
        let row = u16::try_from(i128::from(self.line) - i128::from(origin.line)).ok()?;
        let column = u16::try_from(self.column.checked_sub(origin.column)?).ok()?;
        let position = Position::new(area.x.checked_add(column)?, area.y.checked_add(row)?);
        area.contains(position).then_some(position)
    }
}

/// A run of content cells in reading order. Both ends are included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentRange {
    /// The first cell.
    pub start: ContentPoint,
    /// The last cell.
    pub end: ContentPoint,
}

impl ContentRange {
    /// The range between two points, given in either order.
    #[must_use]
    pub fn new(first: ContentPoint, second: ContentPoint) -> Self {
        Self {
            start: first.min(second),
            end: first.max(second),
        }
    }

    /// Whether `point` is inside the range.
    #[must_use]
    pub fn contains(self, point: ContentPoint) -> bool {
        self.start <= point && point <= self.end
    }

    /// The smallest range that covers both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
