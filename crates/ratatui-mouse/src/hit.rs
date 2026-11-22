//! Frame-local hit areas and pointer clamping.

use ratatui::layout::{Position, Rect};

/// Interactive areas of a frame. Later pushes are on top.
pub struct HitAreas<T> {
    areas: Vec<(Rect, T)>,
}

impl<T> HitAreas<T> {
    /// An empty set of areas.
    #[must_use]
    pub const fn new() -> Self {
        Self { areas: Vec::new() }
    }

    /// Removes every area, keeping the allocation for the next frame.
    pub fn clear(&mut self) {
        self.areas.clear();
    }

    /// Adds an area on top. Empty areas are ignored.
    pub fn push(&mut self, area: Rect, target: T) {
        if !area.is_empty() {
            self.areas.push((area, target));
        }
    }

    /// The topmost target containing `position`.
    #[must_use]
    pub fn target_at(&self, position: Position) -> Option<&T> {
        self.hit_at(position).map(|(_, target)| target)
    }

    /// The topmost target containing `position`, with its area.
    #[must_use]
    pub fn hit_at(&self, position: Position) -> Option<(Rect, &T)> {
        self.areas
            .iter()
            .rev()
            .find(|(area, _)| area.contains(position))
            .map(|(area, target)| (*area, target))
    }

    /// The area most recently pushed for `target`.
    #[must_use]
    pub fn area_of(&self, target: &T) -> Option<Rect>
    where
        T: PartialEq,
    {
        self.areas
            .iter()
            .rev()
            .find(|(_, candidate)| candidate == target)
            .map(|(area, _)| *area)
    }

    /// The areas from the bottom up.
    pub fn iter(&self) -> impl Iterator<Item = (Rect, &T)> {
        self.areas.iter().map(|(area, target)| (*area, target))
    }

    /// Whether no area has been pushed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }
}

impl<T> Default for HitAreas<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A pointer position moved into an area.
pub struct ClampedPosition {
    /// The nearest cell inside the area.
    pub position: Position,
    /// Rows outside the area: negative above, positive below, zero inside.
    pub overshoot_rows: i32,
}

/// Clamps a pointer into an area, counting its outermost margin rows as outside.
///
/// Returns `None` when the area is empty. Overlapping margins use the nearest
/// edge, with ties toward the top. Margins are capped at the visible height.
#[must_use]
pub fn clamp_position_into(
    area: Rect,
    position: Position,
    edge_margin_rows: u16,
) -> Option<ClampedPosition> {
    if area.x >= area.right() || area.y >= area.bottom() {
        return None;
    }
    let last_column = area.right() - 1;
    let last_row = area.bottom() - 1;
    let distance_from_top = i32::from(position.y) - i32::from(area.y);
    let distance_from_bottom = i32::from(last_row) - i32::from(position.y);
    let margin = i32::from(edge_margin_rows.min(area.bottom() - area.y));
    let overshoot_rows = if distance_from_top < margin
        && (distance_from_top <= distance_from_bottom || distance_from_bottom >= margin)
    {
        distance_from_top - margin
    } else if distance_from_bottom < margin {
        margin - distance_from_bottom
    } else {
        0
    };
    Some(ClampedPosition {
        position: Position::new(
            position.x.clamp(area.x, last_column),
            position.y.clamp(area.y, last_row),
        ),
        overshoot_rows,
    })
}
