//! App-owned gesture state with press-time ownership and routing.

use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::{
    ClickCounter, ContentPoint, HitAreas, MouseOutcome, MouseReport, MouseSettings, PointerPress,
    PressRoute, SelectableSurface, Selection, SelectionStage, SelectionUnit, SurfaceSet,
    clamp_position_into, press_route,
};

mod forward;
mod plain;
mod select;
mod timing;

/// Mouse gesture and selection state advanced by events from the app's loop.
/// Targets need only `Clone + PartialEq`; the app owns all surfaces and I/O.
pub struct MouseController<T> {
    settings: MouseSettings,
    gesture: Option<Gesture<T>>,
    selection: Option<Selection<T>>,
    clicks: ClickCounter<T>,
    timing: timing::Timing,
}

enum Gesture<T> {
    Plain(plain::PlainGesture<T>),
    Forward(forward::ForwardGesture<T>),
    Select(select::SelectGesture<T>),
}

impl<T> Gesture<T> {
    const fn target(&self) -> &T {
        match self {
            Self::Plain(gesture) => &gesture.target,
            Self::Forward(gesture) => &gesture.target,
            Self::Select(gesture) => &gesture.target,
        }
    }

    const fn button(&self) -> MouseButton {
        match self {
            Self::Plain(gesture) => gesture.press.button,
            Self::Forward(gesture) => gesture.button,
            Self::Select(gesture) => gesture.press.button,
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
            timing: timing::Timing::new(),
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
        let previous_selection = self.selection_visual();
        self.validate_state(areas, surfaces);
        let report = self.dispatch_event(event, now, areas, surfaces);
        self.complete_report(report, previous_selection != self.selection_visual(), now)
    }

    fn dispatch_event<S: SurfaceSet<T>>(
        &mut self,
        event: &Event,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T> {
        let outcome = match event {
            Event::FocusLost | Event::Resize(_, _) => {
                self.cancel_gesture();
                if matches!(event, Event::Resize(_, _)) && self.settings.should_clear_on_resize {
                    self.clear_selection();
                }
                MouseOutcome::Ignored
            }
            Event::Key(key)
                if self.settings.should_clear_on_key_press
                    && key.kind != KeyEventKind::Release
                    && !matches!(key.code, KeyCode::Modifier(_)) =>
            {
                self.clear_selection();
                MouseOutcome::Ignored
            }
            Event::Mouse(mouse) => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    return self.start_gesture(*mouse, now, areas, surfaces);
                }
                if let Some((rows, columns)) = self.wheel_delta(mouse.kind) {
                    self.handle_wheel(*mouse, (rows, columns), areas, surfaces)
                } else {
                    self.continue_gesture(*mouse, now, areas, surfaces)
                }
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

    /// Copies selected content through the app's borrowed surfaces.
    /// An initial single press has no text until a drag; extension presses do.
    pub fn selected_text<S: SurfaceSet<T>>(&self, surfaces: &mut S) -> Option<String> {
        let selection = self.selection.as_ref()?;
        if selection.stage() == SelectionStage::Pressed
            && selection.unit() == SelectionUnit::Cell
            && !matches!(&self.gesture, Some(Gesture::Select(gesture)) if gesture.is_extension)
        {
            return None;
        }
        Some(
            surfaces
                .surface_mut(selection.target())?
                .text(selection.range()),
        )
    }

    /// Drops the selection and cancels its pending gesture, if any.
    /// Returns whether a selection existed.
    pub fn clear_selection(&mut self) -> bool {
        if matches!(self.gesture, Some(Gesture::Select(_))) {
            self.gesture = None;
            self.timing.edge = None;
        }
        let had_selection = self.selection.take().is_some();
        self.timing.has_pending_redraw |= had_selection;
        had_selection
    }

    /// The target owning the active gesture, if any.
    #[must_use]
    pub fn gesture_owner(&self) -> Option<&T> {
        self.gesture.as_ref().map(Gesture::target)
    }

    /// Cancels without a click or copy and drops any unfinished selection.
    /// Returns whether a gesture existed; completed selections stay intact.
    pub fn cancel_gesture(&mut self) -> bool {
        let gesture = self.gesture.take();
        self.timing.edge = None;
        if matches!(gesture, Some(Gesture::Select(_))) {
            self.timing.has_pending_redraw |= self.selection.take().is_some();
        }
        gesture.is_some()
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
        let origin = surface.as_ref().map(|surface| surface.origin());
        let press = self.create_pointer_press((mouse, button), target, now, area, origin);
        let outcome = self.route_press(target, press, mouse, area, surface);
        create_mouse_report(outcome, Some(target.clone()))
    }

    fn route_press(
        &mut self,
        target: &T,
        press: PointerPress,
        mouse: MouseEvent,
        area: Rect,
        surface: Option<&mut dyn SelectableSurface>,
    ) -> MouseOutcome<T> {
        let request = surface.as_ref().and_then(|surface| surface.pane_request());
        if let Some(request) = request.filter(|request| {
            press_route(request, mouse.modifiers, false, &self.settings) == PressRoute::Forward
        }) {
            self.clear_selection();
            self.gesture = Some(Gesture::Forward(forward::ForwardGesture {
                target: target.clone(),
                button: press.button,
                request,
            }));
            return forward::forward_event(target, mouse, area, request);
        }
        self.gesture = Some(
            if let Some(surface) = surface.filter(|_| press.button == MouseButton::Left) {
                self.start_selection(target, press, surface)
            } else {
                self.clear_selection();
                Gesture::Plain(plain::PlainGesture {
                    target: target.clone(),
                    press,
                    is_dragging: false,
                })
            },
        );
        MouseOutcome::Pressed {
            target: target.clone(),
            press,
        }
    }

    fn create_pointer_press(
        &mut self,
        (mouse, button): (MouseEvent, MouseButton),
        target: &T,
        now: Instant,
        area: Rect,
        origin: Option<ContentPoint>,
    ) -> PointerPress {
        let position = Position::new(mouse.column, mouse.row);
        PointerPress {
            button,
            position,
            modifiers: mouse.modifiers,
            content_point: origin
                .map(|origin| ContentPoint::from_screen_position(position, area, origin)),
            clicks: if button == MouseButton::Left {
                self.clicks
                    .register_press(target, position, now, &self.settings)
            } else {
                1
            },
        }
    }

    fn continue_gesture<S: SurfaceSet<T>>(
        &mut self,
        mouse: MouseEvent,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
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
        if is_drag {
            self.clicks.reset();
        }
        if matches!(gesture, Gesture::Select(_)) {
            self.update_edge(mouse, area, now, is_release);
        }
        let outcome = self.handle_gesture(&mut gesture, mouse, area, is_release, surfaces);
        if !is_release {
            self.gesture = Some(gesture);
        }
        outcome
    }

    fn handle_gesture<S: SurfaceSet<T>>(
        &mut self,
        gesture: &mut Gesture<T>,
        mouse: MouseEvent,
        area: Rect,
        is_release: bool,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let Some(clamped) = clamp_position_into(area, Position::new(mouse.column, mouse.row), 0)
        else {
            return MouseOutcome::Ignored;
        };
        let mouse = MouseEvent {
            column: clamped.position.x,
            row: clamped.position.y,
            ..mouse
        };
        match gesture {
            Gesture::Select(select) => {
                self.continue_selection(select, clamped.position, area, is_release, surfaces)
            }
            Gesture::Plain(plain) => plain.handle_event(clamped.position, is_release),
            Gesture::Forward(forward) => {
                forward::forward_event(&forward.target, mouse, area, forward.request)
            }
        }
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
