//! Demand-driven raw query reader, platform query transport, and event-burst drainer.

#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind};

use crate::json_log::JsonLog;
use crate::parsers::has_da1_reply;
#[cfg(unix)]
use crate::parsers::parse_cursor_position;
#[cfg(unix)]
use crate::sequences::CURSOR_POSITION_QUERY;
use crate::sequences::DA1_FENCE_QUERY;
use crate::session_guard::OutputTarget;
pub use crate::session_guard::{TerminalSessionGuard, TerminalWriter};
use crate::widths::{WidthSample, measured_cell_width};

/// Safety timeout (in milliseconds) when waiting for a `DA1` fence reply (`CSI ? ... c`).
pub const DA1_FENCE_TIMEOUT_MS: u64 = 2_000;

/// Raw bytes collected up to a `DA1` fence or until the fence timeout elapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FencedReply {
    /// Raw terminal reply bytes read during the fenced query window.
    pub bytes: Vec<u8>,
    /// Whether the query timed out before a complete `DA1` reply arrived.
    pub has_timed_out: bool,
}

enum ChunkPollOutcome {
    Bytes(Vec<u8>),
    EndOrTimeout,
}

/// Demand-driven byte reader that reads from an underlying stream only while a query is active.
pub struct DemandByteReader {
    request_sender: Option<SyncSender<()>>,
    reply_receiver: Receiver<io::Result<Vec<u8>>>,
    worker_handle: Option<JoinHandle<()>>,
    has_pending_read: bool,
}

impl std::fmt::Debug for DemandByteReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DemandByteReader")
            .field("has_pending_read", &self.has_pending_read)
            .finish_non_exhaustive()
    }
}

impl DemandByteReader {
    /// Spawns a worker thread that performs one `read` per explicit request token.
    #[must_use]
    pub fn spawn<R>(mut reader: R) -> Self
    where
        R: Read + Send + 'static,
    {
        let (request_sender, request_receiver) = mpsc::sync_channel::<()>(1);
        let (reply_sender, reply_receiver) = mpsc::sync_channel::<io::Result<Vec<u8>>>(1);
        let worker_handle = thread::spawn(move || {
            let mut scratch = [0_u8; 1024];
            // Read only on demand so the worker parks on `request_receiver` immediately once
            // `DA1` arrives and never steals subsequent `OSC 11` or interactive input bytes.
            while request_receiver.recv().is_ok() {
                let read_result = reader
                    .read(&mut scratch)
                    .map(|byte_count| scratch[..byte_count].to_vec());
                if reply_sender.send(read_result).is_err() {
                    break;
                }
            }
        });
        Self {
            request_sender: Some(request_sender),
            reply_receiver,
            worker_handle: Some(worker_handle),
            has_pending_read: false,
        }
    }

    fn poll_next_chunk(&mut self, remaining: Duration) -> io::Result<ChunkPollOutcome> {
        if !self.has_pending_read {
            let Some(sender) = &self.request_sender else {
                return Ok(ChunkPollOutcome::EndOrTimeout);
            };
            if sender.send(()).is_err() {
                return Ok(ChunkPollOutcome::EndOrTimeout);
            }
            self.has_pending_read = true;
        }
        match self.reply_receiver.recv_timeout(remaining) {
            Ok(Ok(chunk)) if chunk.is_empty() => {
                self.has_pending_read = false;
                Ok(ChunkPollOutcome::EndOrTimeout)
            }
            Ok(Ok(chunk)) => {
                self.has_pending_read = false;
                Ok(ChunkPollOutcome::Bytes(chunk))
            }
            Ok(Err(read_error)) => {
                self.has_pending_read = false;
                Err(read_error)
            }
            Err(RecvTimeoutError::Timeout) => Ok(ChunkPollOutcome::EndOrTimeout),
            Err(RecvTimeoutError::Disconnected) => {
                self.has_pending_read = false;
                Ok(ChunkPollOutcome::EndOrTimeout)
            }
        }
    }

    /// Requests reads until [`crate::parsers::has_da1_reply`] matches or `timeout` expires.
    ///
    /// # Errors
    ///
    /// Returns any [`io::Error`] reported by the underlying reader.
    pub fn read_until_da1(&mut self, timeout: Duration) -> io::Result<FencedReply> {
        let deadline = Instant::now() + timeout;
        let mut collected_bytes = Vec::new();
        while !has_da1_reply(&collected_bytes) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(FencedReply {
                    bytes: collected_bytes,
                    has_timed_out: true,
                });
            }
            match self.poll_next_chunk(remaining)? {
                ChunkPollOutcome::Bytes(chunk) => collected_bytes.extend_from_slice(&chunk),
                ChunkPollOutcome::EndOrTimeout => {
                    return Ok(FencedReply {
                        bytes: collected_bytes,
                        has_timed_out: true,
                    });
                }
            }
        }
        Ok(FencedReply {
            bytes: collected_bytes,
            has_timed_out: false,
        })
    }
}

impl Drop for DemandByteReader {
    fn drop(&mut self) {
        drop(self.request_sender.take());
        // Join synchronously when parked on `request_receiver` so `/dev/tty` closes before
        // `crossterm::event` opens it; detach if a timed-out `read` is still blocked.
        if !self.has_pending_read
            && let Some(handle) = self.worker_handle.take()
        {
            let _ = handle.join();
        }
    }
}

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
}

/// Drains immediately available events after `first_event` and counts leading key-press events.
///
/// # Errors
///
/// Propagates any [`io::Error`] returned by `poll_zero`.
pub fn drain_event_burst<F>(first_event: Event, mut poll_zero: F) -> io::Result<(Vec<Event>, usize)>
where
    F: FnMut() -> io::Result<Option<Event>>,
{
    let mut events = vec![first_event];
    while let Some(next_event) = poll_zero()? {
        events.push(next_event);
    }
    let mut burst_press_count = 0;
    for event in &events {
        match event {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                burst_press_count += 1;
            }
            // Windows ConPTY interleaves `Release` after each pasted `Press`; skip `Release`
            // so the leading press burst count matches the pasted character count.
            Event::Key(key_event) if key_event.kind == KeyEventKind::Release => {}
            _ => break,
        }
    }
    Ok((events, burst_press_count))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    struct ScriptedChunkReader {
        chunks: VecDeque<Vec<u8>>,
        reads_performed: Arc<AtomicUsize>,
    }

    impl Read for ScriptedChunkReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads_performed.fetch_add(1, Ordering::SeqCst);
            let Some(next_chunk) = self.chunks.pop_front() else {
                return Ok(0);
            };
            let copy_len = next_chunk.len().min(buffer.len());
            buffer[..copy_len].copy_from_slice(&next_chunk[..copy_len]);
            Ok(copy_len)
        }
    }

    struct BlockingAfterFirstChunkReader {
        first_chunk: Option<Vec<u8>>,
        hang_receiver: mpsc::Receiver<()>,
    }

    impl Read for BlockingAfterFirstChunkReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if let Some(chunk) = self.first_chunk.take() {
                buffer[..chunk.len()].copy_from_slice(&chunk);
                return Ok(chunk.len());
            }
            let _ = self.hang_receiver.recv();
            Ok(0)
        }
    }

    #[test]
    fn drain_event_burst_counts_consecutive_press_keys_and_preserves_trailing_events() {
        // Review Focus #5: paste burst followed by FocusLost reports burst_len == 3 and keeps FocusLost.
        let mut queued = VecDeque::from([
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::FocusLost)),
            Ok(None),
        ]);
        let first_event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let (events, burst_len) =
            drain_event_burst(first_event, || queued.pop_front().unwrap_or(Ok(None)))
                .expect("burst drain must succeed");
        assert_eq!(burst_len, 3);
        assert_eq!(events.len(), 4);
        assert_eq!(events[3], Event::FocusLost);
    }

    #[test]
    fn drain_event_burst_handles_windows_releases_non_key_starts_and_poll_errors() {
        let mut windows_queued = VecDeque::from([
            Ok(Some(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )))),
            Ok(Some(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))),
            Ok(Some(Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
                KeyEventKind::Release,
            )))),
            Ok(Some(Event::Resize(80, 24))),
            Ok(None),
        ]);
        let first_press = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let (events, burst_len) = drain_event_burst(first_press, || {
            windows_queued.pop_front().unwrap_or(Ok(None))
        })
        .expect("windows burst drain must succeed");
        assert_eq!(burst_len, 2);
        assert_eq!(events.len(), 5);

        let (focus_events, focus_burst) = drain_event_burst(Event::FocusGained, || Ok(None))
            .expect("single non-key event must succeed");
        assert_eq!(focus_burst, 0);
        assert_eq!(focus_events, vec![Event::FocusGained]);

        assert!(drain_event_burst(Event::FocusLost, || Err(io::Error::other("poll"))).is_err());
    }

    #[test]
    fn demand_reader_reads_chunked_replies_until_da1_without_stealing_next_bytes() {
        let reads_performed = Arc::new(AtomicUsize::new(0));
        let scripted_reader = ScriptedChunkReader {
            chunks: VecDeque::from([
                b"\x1bP>|xterm.js(6.0.0)\x1b\\".to_vec(),
                b"\x1b[?62;22c".to_vec(),
                b"\x1b]11;rgb:1a/1b/26\x07".to_vec(),
            ]),
            reads_performed: Arc::clone(&reads_performed),
        };
        let mut demand_reader = DemandByteReader::spawn(scripted_reader);
        let fenced_reply = demand_reader
            .read_until_da1(Duration::from_millis(200))
            .expect("chunked DA1 read must succeed");
        assert!(!fenced_reply.has_timed_out);
        assert_eq!(
            fenced_reply.bytes,
            b"\x1bP>|xterm.js(6.0.0)\x1b\\\x1b[?62;22c"
        );
        // Must stop immediately at DA1 so OSC 11 bytes in chunk 3 are never stolen.
        assert_eq!(reads_performed.load(Ordering::SeqCst), 2);
        drop(demand_reader);
    }

    #[test]
    fn demand_reader_times_out_cleanly_when_da1_never_arrives() {
        let (_hang_sender, hang_receiver) = mpsc::channel::<()>();
        let partial_reader = BlockingAfterFirstChunkReader {
            first_chunk: Some(b"\x1b[?1002;1$y".to_vec()),
            hang_receiver,
        };
        let mut demand_reader = DemandByteReader::spawn(partial_reader);
        let fenced_reply = demand_reader
            .read_until_da1(Duration::from_millis(25))
            .expect("timeout should return collected partial bytes");
        assert!(fenced_reply.has_timed_out);
        assert_eq!(fenced_reply.bytes, b"\x1b[?1002;1$y");
        drop(demand_reader);
    }
}
