//! Reusable surface behavior checks.

pub mod surface_contract;

use crossterm::event::{Event, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_mouse::HitAreas;

/// Targets deliberately require neither Copy nor Eq.
#[derive(Debug, Clone, PartialEq)]
pub enum TestTarget {
    /// The left pane.
    Left,
    /// The right pane.
    Right,
    /// A payload that cannot implement Eq, exercising the minimal target bound.
    Other(f64),
}

/// Two nonoverlapping pane content areas.
#[must_use]
pub fn create_areas() -> HitAreas<TestTarget> {
    let mut areas = HitAreas::new();
    areas.push(Rect::new(10, 5, 10, 4), TestTarget::Left);
    areas.push(Rect::new(30, 5, 10, 4), TestTarget::Right);
    areas
}

/// A caller-supplied pointer event.
#[must_use]
pub const fn mouse(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

/// App-like controller fixture retaining frame areas and borrowed surfaces.
pub struct ControllerHarness<S> {
    /// Gesture state under test.
    pub controller: ratatui_mouse::MouseController<TestTarget>,
    /// Current frame areas, editable between events.
    pub areas: HitAreas<TestTarget>,
    /// Borrowed test surfaces.
    pub surfaces: S,
    /// Deterministic event time supplied by the test.
    pub now: std::time::Instant,
}

impl<S: ratatui_mouse::SurfaceSet<TestTarget>> ControllerHarness<S> {
    /// Starts a controller with default settings and the standard pane areas.
    pub fn new(surfaces: S) -> Self {
        Self {
            controller: ratatui_mouse::MouseController::new(ratatui_mouse::MouseSettings::default()),
            areas: create_areas(),
            surfaces,
            now: std::time::Instant::now(),
        }
    }

    /// Passes a mouse event through the public controller API.
    pub fn mouse(
        &mut self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
        modifiers: KeyModifiers,
    ) -> ratatui_mouse::MouseReport<TestTarget> {
        self.event(&mouse(kind, column, row, modifiers))
    }

    /// Passes any event through the public controller API.
    pub fn event(&mut self, event: &Event) -> ratatui_mouse::MouseReport<TestTarget> {
        self.controller
            .handle_event(event, self.now, &self.areas, &mut self.surfaces)
    }
}
