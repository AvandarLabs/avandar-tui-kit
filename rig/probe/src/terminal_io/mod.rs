//! Demand-driven raw query reader, platform query transport, and event-burst drainer.

mod demand_reader;
mod event_burst;

#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, IsTerminal};
use std::time::Duration;

pub use self::demand_reader::{DA1_FENCE_TIMEOUT_MS, DemandByteReader, FencedReply};
pub(crate) use self::event_burst::count_leading_press_burst;
pub use self::event_burst::drain_event_burst;
use crate::json_log::JsonLog;
#[cfg(unix)]
use crate::parsers::parse_cursor_position;
#[cfg(unix)]
use crate::sequences::CURSOR_POSITION_QUERY;
use crate::sequences::DA1_FENCE_QUERY;
use crate::session_guard::OutputTarget;
pub use crate::session_guard::{TerminalSessionGuard, TerminalWriter};
use crate::widths::{WidthSample, measured_cell_width};

/// Active raw query transport used by deterministic capability probe sections.
#[derive(Debug)]
pub struct RawQuerySession {
    writer: TerminalWriter,
    reader: Option<DemandByteReader>,
    has_fence_timed_out: bool,
}

impl RawQuerySession {
    const fn detached() -> Self {
        Self {
            writer: TerminalWriter {
                target: OutputTarget::Detached,
            },
            reader: None,
            has_fence_timed_out: false,
        }
    }

    /// Opens `/dev/tty` (or falls back to standard streams) for raw capability probing.
    #[must_use]
    #[cfg(unix)]
    pub fn open() -> Self {
        // Do not hijack an inherited `/dev/tty` when both standard streams were redirected away.
        if !io::stdin().is_terminal() && !io::stdout().is_terminal() {
            return Self::detached();
        }
        if let Ok(read_file) = OpenOptions::new().read(true).write(true).open("/dev/tty")
            && let Ok(write_file) = read_file.try_clone()
        {
            return Self {
                writer: TerminalWriter {
                    target: OutputTarget::DevTty(write_file),
                },
                reader: Some(DemandByteReader::spawn(read_file)),
                has_fence_timed_out: false,
            };
        }
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            return Self {
                writer: TerminalWriter {
                    target: OutputTarget::Stdout,
                },
                reader: Some(DemandByteReader::spawn(io::stdin())),
                has_fence_timed_out: false,
            };
        }
        Self::detached()
    }

    /// Opens console streams on Windows (raw CSI byte replies are unsupported on ConPTY).
    #[must_use]
    #[cfg(windows)]
    pub fn open() -> Self {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            return Self {
                writer: TerminalWriter {
                    target: OutputTarget::Stdout,
                },
                reader: None,
                has_fence_timed_out: false,
            };
        }
        Self::detached()
    }

    /// Returns `true` when connected to a live TTY or Windows console.
    #[must_use]
    pub const fn is_tty_available(&self) -> bool {
        self.writer.is_tty_available()
    }

    /// Returns `true` when raw CSI/DCS byte replies (`DA1`, `DECRPM`, `XTVERSION`) can be read.
    #[must_use]
    pub const fn is_raw_query_supported(&self) -> bool {
        self.reader.is_some()
    }

    /// Returns `true` if a prior `DA1` fence query timed out on an unresponsive PTY.
    #[must_use]
    pub const fn has_fence_timed_out(&self) -> bool {
        self.has_fence_timed_out
    }

    /// Logs and writes `bytes` without soliciting a `DA1` fence reply.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if logging or terminal output fails.
    pub fn write_raw(&mut self, label: &str, bytes: &[u8], log: &mut JsonLog) -> io::Result<()> {
        self.writer.write_logged(label, bytes, log)
    }

    /// Appends `DA1` (`\x1b[c`) to `query_bytes`, writes them, and reads until `DA1` or timeout.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if terminal output, logging, or raw reply reading fails.
    pub fn query_fenced(
        &mut self,
        label: &str,
        query_bytes: &[u8],
        log: &mut JsonLog,
    ) -> io::Result<FencedReply> {
        if self.reader.is_none() || self.has_fence_timed_out {
            return Ok(FencedReply {
                bytes: Vec::new(),
                has_timed_out: self.has_fence_timed_out,
            });
        }
        let mut fenced_payload = Vec::with_capacity(query_bytes.len() + DA1_FENCE_QUERY.len());
        fenced_payload.extend_from_slice(query_bytes);
        fenced_payload.extend_from_slice(DA1_FENCE_QUERY);
        self.writer.write_logged(label, &fenced_payload, log)?;
        let Some(demand_reader) = &mut self.reader else {
            return Ok(FencedReply {
                bytes: Vec::new(),
                has_timed_out: false,
            });
        };
        let reply = demand_reader.read_until_da1(Duration::from_millis(DA1_FENCE_TIMEOUT_MS))?;
        if reply.has_timed_out {
            self.has_fence_timed_out = true;
        }
        Ok(reply)
    }

    /// Measures the rendered cell width of `sample` on column 1 and clears the probe line.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if terminal I/O or logging fails.
    #[cfg(unix)]
    pub fn measure_sample_width(
        &mut self,
        sample: &WidthSample,
        log: &mut JsonLog,
    ) -> io::Result<(Option<u16>, bool)> {
        if !self.is_raw_query_supported() || self.has_fence_timed_out {
            return Ok((None, self.has_fence_timed_out));
        }
        let mut payload = Vec::with_capacity(12 + sample.text.len());
        payload.extend_from_slice(b"\r\x1b[2K");
        payload.extend_from_slice(sample.text.as_bytes());
        payload.extend_from_slice(CURSOR_POSITION_QUERY);
        let reply = self.query_fenced(&format!("widths:{}", sample.name), &payload, log)?;
        self.write_raw("widths:clear", b"\r\x1b[2K", log)?;
        if reply.has_timed_out {
            return Ok((None, true));
        }
        let cells =
            parse_cursor_position(&reply.bytes).map(|cursor| measured_cell_width(1, cursor.column));
        Ok((cells, false))
    }

    /// Measures the rendered cell width of `sample` on column 1 and clears the probe line.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if terminal I/O or logging fails.
    #[cfg(windows)]
    pub fn measure_sample_width(
        &mut self,
        sample: &WidthSample,
        log: &mut JsonLog,
    ) -> io::Result<(Option<u16>, bool)> {
        if !self.is_tty_available() {
            return Ok((None, false));
        }
        self.write_raw(&format!("widths:{}:reset", sample.name), b"\r\x1b[2K", log)?;
        // `crossterm::cursor::position()` returns 0-based `(column, row)`. Measuring both start
        // and end columns makes the delta independent of 0-based vs 1-based indexing.
        let (start_column, _start_row) = crossterm::cursor::position()?;
        self.write_raw(
            &format!("widths:{}:sample", sample.name),
            sample.text.as_bytes(),
            log,
        )?;
        let (end_column, _end_row) = crossterm::cursor::position()?;
        self.write_raw("widths:clear", b"\r\x1b[2K", log)?;
        Ok((Some(measured_cell_width(start_column, end_column)), false))
    }

    /// Drops the raw byte reader (joining its worker thread) and returns the [`TerminalWriter`].
    #[must_use]
    pub fn into_writer(mut self) -> TerminalWriter {
        drop(self.reader.take());
        self.writer
    }

    #[cfg(test)]
    pub(crate) const fn for_test(reader: DemandByteReader) -> Self {
        Self {
            writer: TerminalWriter {
                target: OutputTarget::Detached,
            },
            reader: Some(reader),
            has_fence_timed_out: false,
        }
    }

    #[cfg(test)]
    pub(crate) const fn detached_for_test() -> Self {
        Self::detached()
    }
}
