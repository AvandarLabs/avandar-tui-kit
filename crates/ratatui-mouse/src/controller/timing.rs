//! Injected-time edge scrolling and coalesced redraws.

use std::time::{Duration, Instant};

use crossterm::event::MouseEvent;
use ratatui::layout::{Position, Rect};

use super::{Gesture, MouseController, create_mouse_report};
use crate::{
    ContentRange, HitAreas, MouseOutcome, MouseReport, SelectionStage, SelectionUnit, SurfaceSet,
    clamp_position_into,
};

pub(super) struct Timing {
    pub edge: Option<EdgeScroll>,
    pub has_pending_redraw: bool,
    last_redraw: Option<Instant>,
}

pub(super) struct EdgeScroll {
    position: Position,
    deadline: Instant,
}

type SelectionVisual<T> = Option<(T, ContentRange, SelectionUnit, SelectionStage)>;

impl Timing {
    pub const fn new() -> Self {
        Self {
            edge: None,
            has_pending_redraw: false,
            last_redraw: None,
        }
    }

    fn redraw_deadline(&self, interval: Duration) -> Option<Instant> {
        self.last_redraw
            .filter(|_| self.has_pending_redraw)?
            .checked_add(interval)
    }

    fn take_redraw(&mut self, now: Instant, interval: Duration) -> bool {
        if self.has_pending_redraw
            && self
                .last_redraw
                .is_none_or(|last| now.saturating_duration_since(last) >= interval)
        {
            self.has_pending_redraw = false;
            self.last_redraw = Some(now);
            true
        } else {
            false
        }
    }
}

impl<T: Clone + PartialEq> MouseController<T> {
    /// Does work due at `now`: one edge-scroll step and a held-back redraw.
    /// Call on each pass of the app's loop, using its last drawn hit areas.
    /// Late calls never replay missed intervals. No clock is read internally.
    pub fn tick<S: SurfaceSet<T>>(
        &mut self,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        let previous_selection = self.selection_visual();
        self.validate_state(areas, surfaces);
        let outcome = self.scroll_edge(now, areas, surfaces);
        self.complete_report(
            create_mouse_report(outcome, None),
            previous_selection != self.selection_visual(),
            now,
        )
    }

    /// The earliest edge-scroll or pending redraw deadline.
    /// The app should wait for input no longer than this moment.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        let edge = self.timing.edge.as_ref().map(|edge| edge.deadline);
        let redraw = self.timing.redraw_deadline(self.settings.redraw_interval);
        match (edge, redraw) {
            (Some(edge), Some(redraw)) => Some(edge.min(redraw)),
            (deadline, None) | (None, deadline) => deadline,
        }
    }

    pub(super) fn selection_visual(&self) -> SelectionVisual<T> {
        self.selection.as_ref().map(|selection| {
            (
                selection.target.clone(),
                selection.range,
                selection.unit,
                selection.stage,
            )
        })
    }

    pub(super) fn complete_report(
        &mut self,
        mut report: MouseReport<T>,
        has_selection_changed: bool,
        now: Instant,
    ) -> MouseReport<T> {
        self.timing.has_pending_redraw |= has_selection_changed;
        report.is_redraw_due = self.timing.take_redraw(now, self.settings.redraw_interval);
        report
    }

    pub(super) fn validate_state<S: SurfaceSet<T>>(
        &mut self,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) {
        if self
            .gesture_owner()
            .is_some_and(|target| areas.area_of(target).is_none())
        {
            self.cancel_gesture();
        }
        if self.selection.as_ref().is_some_and(|selection| {
            areas.area_of(selection.target()).is_none()
                || surfaces.surface_mut(selection.target()).is_none()
        }) {
            self.clear_selection();
        }
    }

    pub(super) fn update_edge(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        now: Instant,
        is_release: bool,
    ) {
        let position = Position::new(mouse.column, mouse.row);
        let is_outside = clamp_position_into(area, position, self.settings.edge_margin_rows)
            .is_some_and(|clamped| clamped.overshoot_rows != 0);
        if is_release || !is_outside || self.settings.auto_scroll_max_rows == 0 {
            self.timing.edge = None;
        } else if let Some(edge) = &mut self.timing.edge {
            edge.position = position;
        } else {
            self.timing.edge = Some(EdgeScroll {
                position,
                deadline: now,
            });
        }
    }

    fn scroll_edge<S: SurfaceSet<T>>(
        &mut self,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        if self
            .timing
            .edge
            .as_ref()
            .is_none_or(|edge| edge.deadline > now)
        {
            return MouseOutcome::Ignored;
        }
        let Some(edge) = self.timing.edge.take() else {
            return MouseOutcome::Ignored;
        };
        let Some(Gesture::Select(gesture)) = self.gesture.as_ref() else {
            return MouseOutcome::Ignored;
        };
        let Some(area) = areas.area_of(&gesture.target) else {
            return MouseOutcome::Ignored;
        };
        let Some(clamped) =
            clamp_position_into(area, edge.position, self.settings.edge_margin_rows)
        else {
            return MouseOutcome::Ignored;
        };
        let maximum = i32::from(self.settings.auto_scroll_max_rows);
        let rows = clamped.overshoot_rows.clamp(-maximum, maximum);
        let Some(surface) = surfaces.surface_mut(&gesture.target) else {
            return MouseOutcome::Ignored;
        };
        let movement = surface.scroll_by(rows);
        if movement == 0 {
            return MouseOutcome::Ignored;
        }
        self.timing.has_pending_redraw = true;
        if movement == rows {
            self.timing.edge = now
                .checked_add(self.settings.auto_scroll_interval)
                .map(|deadline| EdgeScroll { deadline, ..edge });
        }
        self.update_scrolled_selection(clamped.position, area, surfaces)
    }

    pub(super) fn update_scrolled_selection<S: SurfaceSet<T>>(
        &mut self,
        position: Position,
        area: Rect,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let Some(gesture) = self.gesture.take() else {
            return MouseOutcome::Ignored;
        };
        let outcome = if let Gesture::Select(select) = &gesture {
            self.continue_selection(select, position, area, false, surfaces)
        } else {
            MouseOutcome::Ignored
        };
        self.gesture = Some(gesture);
        outcome
    }
}
