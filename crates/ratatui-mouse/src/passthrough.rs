//! Pure routing and encoding for the mouse modes a pane's program requests.

use crossterm::event::KeyModifiers;

use crate::MouseSettings;

mod encoding;
pub use encoding::{build_mouse_report, build_wheel_arrow_keys};

/// Which mouse events a pane's program asked its terminal to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseTracking {
    /// None.
    Off,
    /// Presses only (mode 9).
    Press,
    /// Presses and releases (mode 1000).
    PressRelease,
    /// Presses, releases, and moves with a button held (mode 1002).
    ButtonMotion,
    /// Every move (mode 1003). Served as [`MouseTracking::ButtonMotion`].
    AnyMotion,
}

/// How a pane's program asked for mouse events to be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseEncoding {
    /// Three bytes after `CSI M`.
    Default,
    /// Three UTF-8 characters after `CSI M` (mode 1005).
    Utf8,
    /// Numbers after `CSI <` (mode 1006).
    Sgr,
}

/// What the program in a pane asked its terminal for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneMouseRequest {
    /// Which events it wants.
    pub tracking: MouseTracking,
    /// How it wants them written.
    pub encoding: MouseEncoding,
    /// Whether it is on the alternate screen.
    pub is_alternate_screen: bool,
    /// Whether it turned application cursor keys on.
    pub is_application_cursor: bool,
}

/// Where a press on a pane goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressRoute {
    /// It starts a selection.
    Select,
    /// It is forwarded to the program.
    Forward,
}

/// Where the wheel over a pane goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelRoute {
    /// It is reported to the app.
    Report,
    /// It is forwarded to the program as a wheel event.
    Forward,
    /// It is forwarded to the program as arrow keys.
    ArrowKeys,
}

/// Decides where a press on a pane goes.
#[must_use]
pub fn press_route(
    request: &PaneMouseRequest,
    modifiers: KeyModifiers,
    is_select_mode_on: bool,
    settings: &MouseSettings,
) -> PressRoute {
    if should_select(modifiers, is_select_mode_on, settings)
        || request.tracking == MouseTracking::Off
    {
        PressRoute::Select
    } else {
        PressRoute::Forward
    }
}

/// Decides where the wheel over a pane goes.
#[must_use]
pub fn wheel_route(
    request: &PaneMouseRequest,
    modifiers: KeyModifiers,
    is_select_mode_on: bool,
    settings: &MouseSettings,
) -> WheelRoute {
    if should_select(modifiers, is_select_mode_on, settings) {
        WheelRoute::Report
    } else if request.tracking != MouseTracking::Off {
        WheelRoute::Forward
    } else if request.is_alternate_screen && settings.is_wheel_to_arrows_enabled {
        WheelRoute::ArrowKeys
    } else {
        WheelRoute::Report
    }
}

const fn should_select(
    modifiers: KeyModifiers,
    is_select_mode_on: bool,
    settings: &MouseSettings,
) -> bool {
    is_select_mode_on
        || modifiers.intersects(settings.selection_modifiers)
        || !settings.is_passthrough_enabled
}
