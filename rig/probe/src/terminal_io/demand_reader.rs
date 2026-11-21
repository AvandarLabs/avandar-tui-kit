//! Worker-thread demand-driven byte reader synchronized by `DA1` fences.

use std::io::{self, Read};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::parsers::has_da1_reply;

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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
