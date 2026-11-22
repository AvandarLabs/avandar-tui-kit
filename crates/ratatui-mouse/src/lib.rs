//! Mouse reporting and interaction building blocks for ratatui apps using crossterm.
//!
//! The caller owns the event loop, drawing stream, and panic hook. Reporting
//! setup and teardown are available through [`modes`] or the crate root. Frame
//! hit areas, content coordinates, click runs, and settings are pure app-owned parts.

pub mod clicks;
pub mod content;
mod error;
pub mod hit;
pub mod modes;
pub mod settings;

pub use clicks::ClickCounter;
pub use content::{ContentPoint, ContentRange};
pub use error::MouseError;
pub use hit::{ClampedPosition, HitAreas, clamp_position_into};
pub use modes::{
    DISABLE_MOUSE_REPORTING, ENABLE_MOUSE_REPORTING, MouseReporting, disable_mouse_reporting,
    enable_mouse_reporting,
};
pub use settings::{MouseSettings, WordCharacters};
