//! Typed reporting errors that retain the underlying I/O cause.

use std::{error::Error, fmt, io};

/// What can go wrong in this crate.
#[derive(Debug)]
#[non_exhaustive]
pub enum MouseError {
    /// Mouse reporting could not be turned on.
    EnableReporting(io::Error),
    /// Mouse reporting could not be turned off.
    DisableReporting(io::Error),
}

impl fmt::Display for MouseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnableReporting(error) => {
                write!(formatter, "Could not enable mouse reporting: {error}")
            }
            Self::DisableReporting(error) => {
                write!(formatter, "Could not disable mouse reporting: {error}")
            }
        }
    }
}

impl Error for MouseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EnableReporting(error) | Self::DisableReporting(error) => Some(error),
        }
    }
}
