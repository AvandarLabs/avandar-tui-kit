//! Reporting setup, explicit teardown, and best-effort cleanup on drop.

use std::io::Write;

use crate::MouseError;

mod platform;

/// Enables press/release (1000), button-motion (1002), and SGR coordinates
/// (1006), in order. Hover reporting (1003) is never enabled.
pub const ENABLE_MOUSE_REPORTING: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

/// Disables every mouse tracking mode and coordinate format.
///
/// These are the teardown bytes for a macOS or Linux panic hook. On Windows,
/// call [`disable_mouse_reporting`] to restore the console input mode instead.
pub const DISABLE_MOUSE_REPORTING: &[u8] =
    b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";

/// Turns reporting on using the stream the app draws on.
///
/// On macOS and Linux, writes [`ENABLE_MOUSE_REPORTING`] and flushes `writer`.
/// On Windows, calls crossterm's console API without writing or flushing the
/// supplied writer. Crossterm remembers the first input mode process-wide;
/// reporting guards are not a nested restoration stack.
///
/// # Errors
///
/// Returns [`MouseError::EnableReporting`] if writing, flushing, or the console
/// call fails. A failed enable attempts best-effort teardown before returning
/// the original error, since reporting might already be partially enabled.
pub fn enable_mouse_reporting(writer: &mut impl Write) -> Result<(), MouseError> {
    if let Err(error) = platform::enable(writer) {
        let _ = platform::disable(writer);
        return Err(MouseError::EnableReporting(error));
    }
    Ok(())
}

/// Turns reporting off. It is safe to call repeatedly or before enabling.
///
/// On macOS and Linux, writes [`DISABLE_MOUSE_REPORTING`] and flushes `writer`.
/// On Windows, restores crossterm's first saved console input mode and never
/// writes or flushes `writer`. Calling before crossterm has saved a mode may
/// return an error; it does not panic.
///
/// # Errors
///
/// Returns [`MouseError::DisableReporting`] if writing, flushing, or restoring
/// the console input mode fails.
pub fn disable_mouse_reporting(writer: &mut impl Write) -> Result<(), MouseError> {
    platform::disable(writer).map_err(MouseError::DisableReporting)
}

/// Mouse reporting that is on for as long as this value lives.
///
/// Drop performs best-effort teardown and ignores errors. Use
/// [`Self::disable`] to receive a cleanup error. The caller installs any panic
/// hook and must use the same drawing stream for hook teardown.
pub struct MouseReporting<W: Write> {
    writer: W,
    is_enabled: bool,
}

impl<W: Write> MouseReporting<W> {
    /// Enables reporting on the stream the app draws on and returns a guard.
    ///
    /// # Errors
    ///
    /// Returns [`MouseError::EnableReporting`] if reporting could not be enabled.
    pub fn enable(mut writer: W) -> Result<Self, MouseError> {
        enable_mouse_reporting(&mut writer)?;
        Ok(Self {
            writer,
            is_enabled: true,
        })
    }

    /// Turns reporting off now and reports a failure.
    ///
    /// A failed explicit cleanup is retried once by drop. Successful explicit
    /// cleanup prevents a second teardown when the guard is dropped.
    ///
    /// # Errors
    ///
    /// Returns [`MouseError::DisableReporting`] if reporting could not be disabled.
    pub fn disable(mut self) -> Result<(), MouseError> {
        disable_mouse_reporting(&mut self.writer)?;
        self.is_enabled = false;
        Ok(())
    }
}

impl<W: Write> Drop for MouseReporting<W> {
    fn drop(&mut self) {
        if self.is_enabled {
            let _ = disable_mouse_reporting(&mut self.writer);
        }
    }
}
