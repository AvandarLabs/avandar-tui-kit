//! App-owned gesture state with press-time ownership and routing.

use std::time::Instant;

use crossterm::event::{Event, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::{
    ClickCounter, ContentPoint, HitAreas, MouseOutcome, MouseReport, MouseSettings, PointerPress,
    PressRoute, SelectableSurface, Selection, SurfaceSet, clamp_position_into, press_route,
};

mod forward;
mod plain;

/// Mouse gesture and selection state advanced by events from the app's loop.
/// Targets need only `Clone + PartialEq`; the app owns all surfaces and I/O.
pub struct MouseController<T> {
    settings: MouseSettings,
    gesture: Option<Gesture<T>>,
    selection: Option<Selection<T>>,
    clicks: ClickCounter<T>,
}

enum Gesture<T> {
    Plain(plain::PlainGesture<T>),
    Forward(forward::ForwardGesture<T>),
}

impl<T> Gesture<T> {
    const fn target(&self) -> &T {
        match self {
            Self::Plain(gesture) => &gesture.target,
            Self::Forward(gesture) => &gesture.target,
        }
    }

    const fn button(&self) -> MouseButton {
        match self {
            Self::Plain(gesture) => gesture.press.button,
            Self::Forward(gesture) => gesture.button,
        }
    }
}

impl<T: Clone + PartialEq> MouseController<T> {
    /// Creates a controller without a gesture or selection.
    #[must_use]
    pub const fn new(settings: MouseSettings) -> Self {
        Self {
            settings,
            gesture: None,
            selection: None,
            clicks: ClickCounter::new(),
        }
    }

    /// The settings in use.
    #[must_use]
    pub const fn settings(&self) -> &MouseSettings {
        &self.settings
    }

    /// The settings, to change between events.
    pub const fn settings_mut(&mut self) -> &mut MouseSettings {
        &mut self.settings
    }

    /// Advances mouse state using app-supplied time and the last drawn areas.
    ///
    /// The press owns all drags and its matching release. Its area is looked
    /// up on every event; a missing owner, focus loss, or resize cancels it.
    /// Bare motion and releases without a matching press are ignored.
    /// Forwarding retains the press-time pane request through the release.
    pub fn handle_event<S: SurfaceSet<T>>(
        &mut self,
        event: &Event,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        if self
            .gesture_owner()
            .is_some_and(|target| areas.area_of(target).is_none())
        {
            self.cancel_gesture();
        }
        let outcome = match event {
            Event::FocusLost | Event::Resize(_, _) => {
                self.cancel_gesture();
                MouseOutcome::Ignored
            }
            Event::Mouse(mouse) => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    return self.start_gesture(*mouse, now, areas, surfaces);
                }
                self.continue_gesture(*mouse, areas)
            }
            _ => MouseOutcome::Ignored,
        };
        create_mouse_report(outcome, None)
    }

    /// The selection to highlight, if any.
    #[must_use]
    pub const fn selection(&self) -> Option<&Selection<T>> {
        self.selection.as_ref()
    }

    /// Copies the selected content through the app's borrowed surfaces.
    pub fn selected_text<S: SurfaceSet<T>>(&self, surfaces: &mut S) -> Option<String> {
        let selection = self.selection.as_ref()?;
        Some(
            surfaces
                .surface_mut(selection.target())?
                .text(selection.range()),
        )
    }

    /// Drops the selection, returning whether one existed.
    pub fn clear_selection(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// The target owning the active gesture, if any.
    #[must_use]
    pub fn gesture_owner(&self) -> Option<&T> {
        self.gesture.as_ref().map(Gesture::target)
    }

    /// Cancels without a click or copy, returning whether a gesture existed.
    pub fn cancel_gesture(&mut self) -> bool {
        self.gesture.take().is_some()
    }

    /// Drops state for a target that has gone away, returning whether any existed.
    pub fn forget_target(&mut self, target: &T) -> bool {
        let has_click_history = self.clicks.forget_target(target);
        let has_gesture = self.gesture_owner() == Some(target);
        let has_selection = self
            .selection
            .as_ref()
            .is_some_and(|selection| selection.target() == target);
        if has_gesture {
            self.cancel_gesture();
        }
        if has_selection {
            self.clear_selection();
        }
        has_gesture || has_selection || has_click_history
    }

    fn start_gesture<S: SurfaceSet<T>>(
        &mut self,
        mouse: MouseEvent,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        self.cancel_gesture();
        let position = Position::new(mouse.column, mouse.row);
        let Some((area, target)) = areas.hit_at(position) else {
            self.clear_selection();
            return create_mouse_report(MouseOutcome::Ignored, None);
        };
        let MouseEventKind::Down(button) = mouse.kind else {
            return create_mouse_report(MouseOutcome::Ignored, None);
        };
        let surface = surfaces.surface_mut(target);
        let request = surface.as_ref().and_then(|surface| surface.pane_request());
        let press = self.create_pointer_press((mouse, button), target, now, area, surface);
        let outcome = if let Some(request) = request.filter(|request| {
            press_route(request, mouse.modifiers, false, &self.settings) == PressRoute::Forward
        }) {
            self.gesture = Some(Gesture::Forward(forward::ForwardGesture {
                target: target.clone(),
                button,
                request,
            }));
            forward::forward_event(target, mouse, area, request)
        } else {
            self.gesture = Some(Gesture::Plain(plain::PlainGesture {
                target: target.clone(),
                press,
                is_dragging: false,
            }));
            MouseOutcome::Pressed {
                target: target.clone(),
                press,
            }
        };
        create_mouse_report(outcome, Some(target.clone()))
    }

    fn create_pointer_press(
        &mut self,
        (mouse, button): (MouseEvent, MouseButton),
        target: &T,
        now: Instant,
        area: Rect,
        surface: Option<&mut dyn SelectableSurface>,
    ) -> PointerPress {
        let position = Position::new(mouse.column, mouse.row);
        PointerPress {
            button,
            position,
            modifiers: mouse.modifiers,
            content_point: surface.map(|surface| {
                ContentPoint::from_screen_position(position, area, surface.origin())
            }),
            clicks: if button == MouseButton::Left {
                self.clicks
                    .register_press(target, position, now, &self.settings)
            } else {
                1
            },
        }
    }

    fn continue_gesture(&mut self, mouse: MouseEvent, areas: &HitAreas<T>) -> MouseOutcome<T> {
        let Some(mut gesture) = self.gesture.take() else {
            return MouseOutcome::Ignored;
        };
        let is_drag =
            matches!(mouse.kind, MouseEventKind::Drag(button) if button == gesture.button());
        let is_release =
            matches!(mouse.kind, MouseEventKind::Up(button) if button == gesture.button());
        if !is_drag && !is_release {
            self.gesture = Some(gesture);
            return MouseOutcome::Ignored;
        }
        let Some(area) = areas.area_of(gesture.target()) else {
            return MouseOutcome::Ignored;
        };
        let Some(clamped) = clamp_position_into(area, Position::new(mouse.column, mouse.row), 0)
        else {
            return MouseOutcome::Ignored;
        };
        if is_drag {
            self.clicks.reset();
        }
        let mouse = MouseEvent {
            column: clamped.position.x,
            row: clamped.position.y,
            ..mouse
        };
        let outcome = match &mut gesture {
            Gesture::Plain(plain) => plain.handle_event(clamped.position, is_release),
            Gesture::Forward(forward) => {
                forward::forward_event(&forward.target, mouse, area, forward.request)
            }
        };
        if !is_release {
            self.gesture = Some(gesture);
        }
        outcome
    }
}

const fn create_mouse_report<T>(
    outcome: MouseOutcome<T>,
    pressed_target: Option<T>,
) -> MouseReport<T> {
    MouseReport {
        outcome,
        pressed_target,
        is_redraw_due: false,
    }
}
