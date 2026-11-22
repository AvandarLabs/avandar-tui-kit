//! Mouse reporting building blocks for ratatui apps using crossterm.
//!
//! The caller owns the event loop, drawing stream, and panic hook. Reporting
//! setup and teardown are available through [`modes`] or the crate root.

mod error;
pub mod modes;

pub use error::MouseError;
pub use modes::{
    DISABLE_MOUSE_REPORTING, ENABLE_MOUSE_REPORTING, MouseReporting, disable_mouse_reporting,
    enable_mouse_reporting,
};
