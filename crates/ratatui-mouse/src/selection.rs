//! Read-only selection state owned by the mouse controller.

use crate::ContentRange;

/// What a selection grows by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionUnit {
    /// Cells: a press and a drag.
    Cell,
    /// Whole words: a double click.
    Word,
    /// Whole lines: a triple click.
    Line,
}

/// How far along a selection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStage {
    /// The button is down and the pointer has not moved.
    Pressed,
    /// The pointer has moved with the button down.
    Dragging,
    /// The button came up. The selection stays until it is cleared.
    Done,
}

/// Selected text on one target. A selection never spans targets.
pub struct Selection<T> {
    pub(crate) target: T,
    pub(crate) anchor: ContentRange,
    pub(crate) range: ContentRange,
    pub(crate) unit: SelectionUnit,
    pub(crate) stage: SelectionStage,
}

impl<T> Selection<T> {
    /// The target the selection is on.
    #[must_use]
    pub const fn target(&self) -> &T {
        &self.target
    }

    /// The selected cells.
    #[must_use]
    pub const fn range(&self) -> ContentRange {
        self.range
    }

    /// What the selection grows by.
    #[must_use]
    pub const fn unit(&self) -> SelectionUnit {
        self.unit
    }

    /// How far along the selection is.
    #[must_use]
    pub const fn stage(&self) -> SelectionStage {
        self.stage
    }
}
