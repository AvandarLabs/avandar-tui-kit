//! Reports for the app to act on after mouse interaction.

use crossterm::event::{KeyModifiers, MouseButton};
use ratatui::layout::Position;

use crate::ContentPoint;

/// What one call to the controller produced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct MouseReport<T> {
    /// What happened.
    pub outcome: MouseOutcome<T>,
    /// The target a button went down on in this event, whatever else the press did.
    pub pressed_target: Option<T>,
    /// Whether the app should draw a frame now.
    pub is_redraw_due: bool,
}

/// A button press on a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerPress {
    /// The button.
    pub button: MouseButton,
    /// Where on screen.
    pub position: Position,
    /// Where in the target's content, when it has a surface.
    pub content_point: Option<ContentPoint>,
    /// 1, 2 or 3 for a single, double or triple click.
    pub clicks: u8,
    /// The modifiers held.
    pub modifiers: KeyModifiers,
}

/// What happened, for the app to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseOutcome<T> {
    /// The event is not the crate's. Handle it as usual.
    Ignored,
    /// The crate handled the event and there is nothing to act on.
    Consumed,
    /// A button went down on `target`.
    Pressed {
        /// The target owning the press.
        target: T,
        /// The button press details.
        press: PointerPress,
    },
    /// A button went down and came up on `target` with no drag between.
    Clicked {
        /// The target owning the click.
        target: T,
        /// The original button press details.
        press: PointerPress,
    },
    /// The pointer moved during a plain gesture owned by `target`.
    Dragged {
        /// The target owning the gesture.
        target: T,
        /// The held button.
        button: MouseButton,
        /// The position where the gesture started.
        from: Position,
        /// The current pointer position.
        to: Position,
    },
    /// A plain gesture that dragged was released.
    DragEnded {
        /// The target owning the gesture.
        target: T,
        /// The released button.
        button: MouseButton,
        /// The position where the gesture started.
        from: Position,
        /// The pointer position at release.
        to: Position,
    },
    /// An unused wheel event. Rows and columns are negative toward the start.
    Wheel {
        /// The target under the wheel.
        target: T,
        /// The signed vertical wheel movement.
        rows: i32,
        /// The signed horizontal wheel movement.
        columns: i32,
        /// The modifiers held.
        modifiers: KeyModifiers,
    },
    /// Selected text for the app to send to the clipboard.
    Copy {
        /// The target owning the selection.
        target: T,
        /// The original selected text.
        text: String,
    },
    /// Bytes for the app to write to a program in a pane.
    Forward {
        /// The pane target receiving the bytes.
        target: T,
        /// The encoded mouse input.
        bytes: Vec<u8>,
    },
}
