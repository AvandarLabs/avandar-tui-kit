//! Selectable text surfaces borrowed from app-owned state.

use std::ops::RangeInclusive;

use crate::{ContentPoint, ContentRange, PaneMouseRequest, WordCharacters};

mod line;
pub use line::LineSurface;

/// Something text can be selected in.
pub trait SelectableSurface {
    /// The content point at the viewport's top left.
    fn origin(&self) -> ContentPoint;
    /// The first and last content line that exist; empty when there are no rows.
    fn line_bounds(&self) -> RangeInclusive<i64>;
    /// The cells of the character, including both cells of wide characters.
    fn character_at(&mut self, point: ContentPoint) -> ContentRange;
    /// The cells of the word, or none on blanks or non-word characters.
    fn word_at(
        &mut self,
        point: ContentPoint,
        word_characters: &WordCharacters,
    ) -> Option<ContentRange>;
    /// The whole logical line across its soft-wrapped rows.
    fn line_at(&mut self, line: i64) -> ContentRange;
    /// Copy the inclusive range without changing the scroll position.
    /// Join soft-wrapped rows directly and other rows with newlines, trim
    /// trailing blanks on logical lines, and keep wide characters whole.
    /// Copy ranges taller than the viewport in full, without a trailing newline.
    fn text(&mut self, range: ContentRange) -> String;
    /// Scroll by rows and return the actual signed movement.
    fn scroll_by(&mut self, rows: i32) -> i32;
    /// The mouse modes requested by a terminal pane, if any.
    fn pane_request(&self) -> Option<PaneMouseRequest> {
        None
    }
}

/// The app's surfaces, looked up by target.
pub trait SurfaceSet<T> {
    /// The surface behind the target, or none when it has no selectable text.
    fn surface_mut(&mut self, target: &T) -> Option<&mut dyn SelectableSurface>;
}

/// The surface set of an app with nothing to select.
pub struct NoSurfaces;
impl<T> SurfaceSet<T> for NoSurfaces {
    fn surface_mut(&mut self, _: &T) -> Option<&mut dyn SelectableSurface> {
        None
    }
}

/// Where an app-held view is scrolled to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollPosition {
    /// The row shown at the top.
    pub line: usize,
    /// The column shown at the left edge, in cells.
    pub column: u32,
}
