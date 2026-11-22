//! Forwarding uses the owner's current area and press-time pane request.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{Gesture, MouseController};
use crate::{
    HitAreas, MouseOutcome, PaneMouseRequest, SurfaceSet, WheelRoute, build_mouse_report,
    build_wheel_arrow_keys, clamp_position_into, wheel_route,
};

pub(super) struct ForwardGesture<T> {
    pub target: T,
    pub button: MouseButton,
    pub request: PaneMouseRequest,
}

pub(super) fn forward_event<T: Clone>(
    target: &T,
    mouse: MouseEvent,
    area: Rect,
    request: PaneMouseRequest,
) -> MouseOutcome<T> {
    build_mouse_report(&mouse, area, &request).map_or(MouseOutcome::Consumed, |bytes| {
        MouseOutcome::Forward {
            target: target.clone(),
            bytes,
        }
    })
}

impl<T: Clone + PartialEq> MouseController<T> {
    pub(super) fn wheel_delta(&self, kind: MouseEventKind) -> Option<(i32, i32)> {
        use crossterm::event::MouseEventKind::{ScrollDown, ScrollLeft, ScrollRight, ScrollUp};
        let step = i32::from(self.settings.wheel_step_rows);
        match kind {
            ScrollUp => Some((-step, 0)),
            ScrollDown => Some((step, 0)),
            ScrollLeft => Some((0, -step)),
            ScrollRight => Some((0, step)),
            _ => None,
        }
    }

    pub(super) fn handle_wheel<S: SurfaceSet<T>>(
        &mut self,
        mouse: MouseEvent,
        (rows, columns): (i32, i32),
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let position = Position::new(mouse.column, mouse.row);
        if let Some(gesture) = self
            .gesture
            .as_ref()
            .filter(|gesture| !matches!(gesture, Gesture::Plain(_)))
        {
            let Some(area) = areas.area_of(gesture.target()) else {
                return MouseOutcome::Ignored;
            };
            let Some(clamped) = clamp_position_into(area, position, 0) else {
                return MouseOutcome::Ignored;
            };
            if let Gesture::Forward(forward) = gesture {
                return forward_event(
                    &forward.target,
                    MouseEvent {
                        column: clamped.position.x,
                        row: clamped.position.y,
                        ..mouse
                    },
                    area,
                    forward.request,
                );
            }
            if let Some(surface) = surfaces.surface_mut(gesture.target()) {
                self.timing.has_pending_redraw |= surface.scroll_by(rows) != 0;
            }
            self.refresh_edge_pointer(mouse, area);
            return self.update_scrolled_selection(clamped.position, area, surfaces);
        }
        self.handle_hovered_wheel(mouse, (rows, columns), areas, surfaces)
    }

    fn handle_hovered_wheel<S: SurfaceSet<T>>(
        &self,
        mouse: MouseEvent,
        (rows, columns): (i32, i32),
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseOutcome<T> {
        let position = Position::new(mouse.column, mouse.row);
        let Some((area, target)) = areas.hit_at(position) else {
            return MouseOutcome::Ignored;
        };
        let request = surfaces
            .surface_mut(target)
            .and_then(|surface| surface.pane_request());
        if let Some(outcome) =
            request.and_then(|request| self.route_wheel(target, mouse, area, request, rows))
        {
            return outcome;
        }
        MouseOutcome::Wheel {
            target: target.clone(),
            rows,
            columns,
            modifiers: mouse.modifiers,
        }
    }

    fn route_wheel(
        &self,
        target: &T,
        mouse: MouseEvent,
        area: Rect,
        request: PaneMouseRequest,
        rows: i32,
    ) -> Option<MouseOutcome<T>> {
        match wheel_route(
            &request,
            mouse.modifiers,
            self.select_mode_target() == Some(target),
            &self.settings,
        ) {
            WheelRoute::Forward => Some(forward_event(target, mouse, area, request)),
            WheelRoute::ArrowKeys if rows != 0 => Some(MouseOutcome::Forward {
                target: target.clone(),
                bytes: build_wheel_arrow_keys(rows, request.is_application_cursor),
            }),
            _ => None,
        }
    }
}
