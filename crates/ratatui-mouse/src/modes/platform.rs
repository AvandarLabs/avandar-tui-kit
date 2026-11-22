//! The sole boundary for stream writes and Windows console operations.

use std::io::{self, Write};

#[cfg(not(windows))]
use super::{DISABLE_MOUSE_REPORTING, ENABLE_MOUSE_REPORTING};

#[cfg(not(windows))]
pub(super) fn enable(writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(ENABLE_MOUSE_REPORTING)?;
    writer.flush()
}

#[cfg(not(windows))]
pub(super) fn disable(writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(DISABLE_MOUSE_REPORTING)?;
    writer.flush()
}

#[cfg(windows)]
pub(super) fn enable(_writer: &mut impl Write) -> io::Result<()> {
    use crossterm::{Command, event::EnableMouseCapture};

    EnableMouseCapture.execute_winapi()
}

#[cfg(windows)]
pub(super) fn disable(_writer: &mut impl Write) -> io::Result<()> {
    use crossterm::{Command, event::DisableMouseCapture};

    DisableMouseCapture.execute_winapi()
}
