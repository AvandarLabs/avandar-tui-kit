//! Plain gestures retain their original press through clamped drags.

use ratatui::layout::Position;

use crate::{MouseOutcome, PointerPress};

pub(super) struct PlainGesture<T> {
    pub target: T,
    pub press: PointerPress,
    pub is_dragging: bool,
}

impl<T: Clone> PlainGesture<T> {
    pub fn handle_event(&mut self, position: Position, is_release: bool) -> MouseOutcome<T> {
        if is_release && !self.is_dragging {
            return MouseOutcome::Clicked {
                target: self.target.clone(),
                press: self.press,
            };
        }
        self.is_dragging = true;
        if is_release {
            MouseOutcome::DragEnded {
                target: self.target.clone(),
                button: self.press.button,
                from: self.press.position,
                to: position,
            }
        } else {
            MouseOutcome::Dragged {
                target: self.target.clone(),
                button: self.press.button,
                from: self.press.position,
                to: position,
            }
        }
    }
}
