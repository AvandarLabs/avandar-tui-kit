//! Mouse reporting and interaction building blocks for ratatui apps using crossterm.
//!
//! The caller owns the event loop, drawing stream, and panic hook. Reporting
//! setup and teardown are available through [`modes`] or the crate root. Frame
//! hit areas, content coordinates, click runs, settings, and selection highlights
//! are pure app-owned parts. Reports leave clipboard and pane I/O to the app.
//!
//! Keep the most recently drawn [`HitAreas`], pass events and an explicit
//! [`std::time::Instant`] to [`MouseController::handle_event`], bound input waits
//! by [`MouseController::next_deadline`], and call [`MouseController::tick`] on
//! every loop pass. Bind an app command for [`MouseController::enter_select_mode`]:
//! v plus motions selects, W selects a word, V selects lines, y copies, and Esc
//! clears or leaves. The app also binds focus, activation, scrolling and resizing.
//!
//! [`SelectableSurface`] implementations use inclusive cell coordinates, copy
//! whole wide characters and offscreen ranges, join soft wraps, trim logical-line
//! trailing blanks, and preserve their viewport during text lookup. No default
//! feature is enabled; `vt100` adds a borrowed emulator surface. Reborrow it after
//! output or resize, and clear stale selections when retained history is evicted.
//!
//! Shift and Alt select instead of forwarding when delivered by the host.
//! xterm.js on Linux/Windows keeps Shift for native selection; iTerm2 keeps
//! Option (Option+Command for rectangles), while Command arrives as Alt.
//! Stock and full tmux forward requested pane events, but tmux 3.8 drops Alt-drag
//! motion and tmux 3.3a can absorb rapid right/Control presses. Select mode remains
//! the complete keyboard path. Native selection copies screen cells through the
//! host; in-app selection returns surface text through [`MouseOutcome::Copy`].
//!
//! Unix setup never requests hover. Windows ConPTY requests any-motion plus SGR,
//! and bare moves are ignored. Pane any-motion requests receive button motion
//! only. Query silence (ConPTY/tmux 3.3a) does not establish reset. Wheel direction
//! is used as received, including macOS natural-scrolling inversion. The crate
//! README documents the full terminal limits and the `mouse_lab` example.

pub mod clicks;
pub mod content;
pub mod controller;
mod error;
pub mod highlight;
pub mod hit;
pub mod modes;
pub mod outcome;
pub mod passthrough;
pub mod select_mode;
pub mod selection;
pub mod settings;
pub mod surface;
#[cfg(feature = "vt100")]
pub mod vt100;
pub mod words;

#[cfg(feature = "vt100")]
pub use vt100::Vt100Surface;

pub use clicks::ClickCounter;
pub use content::{ContentPoint, ContentRange};
pub use controller::MouseController;
pub use error::MouseError;
pub use highlight::{DEFAULT_HIGHLIGHT_STYLE, HighlightRects};
pub use hit::{ClampedPosition, HitAreas, clamp_position_into};
pub use modes::{
    DISABLE_MOUSE_REPORTING, ENABLE_MOUSE_REPORTING, MouseReporting, disable_mouse_reporting,
    enable_mouse_reporting,
};
pub use outcome::{MouseOutcome, MouseReport, PointerPress};
pub use selection::{Selection, SelectionStage, SelectionUnit};
pub use settings::{MouseSettings, WordCharacters};

pub use passthrough::{
    MouseEncoding, MouseTracking, PaneMouseRequest, PressRoute, WheelRoute, build_mouse_report,
    build_wheel_arrow_keys, press_route, wheel_route,
};
pub use surface::{LineSurface, NoSurfaces, ScrollPosition, SelectableSurface, SurfaceSet};
pub use words::{character_columns_in_line, text_in_columns, word_columns_in_line};

pub use select_mode::{SelectionCommand, SelectionMotion, selection_command_from_key};
