//! Forwarding uses the owner's current area and press-time pane request.

use crossterm::event::{MouseButton, MouseEvent};
use ratatui::layout::Rect;

use crate::{MouseOutcome, PaneMouseRequest, build_mouse_report};

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
