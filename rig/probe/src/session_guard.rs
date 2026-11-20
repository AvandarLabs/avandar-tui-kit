//! Terminal session guard (`Drop` + once-only panic hook) and logged terminal writer.

#[cfg(unix)]
use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::json_log::JsonLog;
use crate::section_json::format_write_record;
#[cfg(unix)]
use crate::sequences::MOUSE_SETUP_M1;

#[derive(Debug, Default)]
struct SharedSessionState {
    has_active_session: AtomicBool,
    is_raw_mode_enabled: AtomicBool,
    is_kitty_pushed: AtomicBool,
    #[cfg(windows)]
    is_windows_mouse_captured: AtomicBool,
}

impl SharedSessionState {
    fn restore_best_effort(&self) {
        // Atomically claim restoration so panic-hook + unwind `Drop` never pop Kitty stack twice.
        if !self.has_active_session.swap(false, Ordering::AcqRel) {
            return;
        }
        let was_kitty_pushed = self.is_kitty_pushed.swap(false, Ordering::AcqRel);
        let was_raw_mode_enabled = self.is_raw_mode_enabled.swap(false, Ordering::AcqRel);
        let reset_bytes = crate::probe_runner::build_teardown_bytes(was_kitty_pushed);
        self.write_teardown_sequences(&reset_bytes);
        if was_raw_mode_enabled {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }

    #[cfg(unix)]
    #[expect(
        clippy::unused_self,
        reason = "matches Windows signature which reads is_windows_mouse_captured from self"
    )]
    fn write_teardown_sequences(&self, reset_bytes: &[u8]) {
        if let Ok(mut tty_file) = OpenOptions::new().write(true).open("/dev/tty") {
            let _ = tty_file.write_all(reset_bytes);
            let _ = tty_file.flush();
        } else if io::stdout().is_terminal() {
            let mut stdout_handle = io::stdout();
            let _ = stdout_handle.write_all(reset_bytes);
            let _ = stdout_handle.flush();
        }
    }

    #[cfg(windows)]
    fn write_teardown_sequences(&self, reset_bytes: &[u8]) {
        if self.is_windows_mouse_captured.swap(false, Ordering::AcqRel) {
            let _ = crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture);
        }
        if io::stdout().is_terminal() {
            let mut stdout_handle = io::stdout();
            let _ = stdout_handle.write_all(reset_bytes);
            let _ = stdout_handle.flush();
        }
    }
}

static PANIC_HOOK_STATE: OnceLock<Arc<SharedSessionState>> = OnceLock::new();

fn shared_session_state() -> &'static Arc<SharedSessionState> {
    PANIC_HOOK_STATE.get_or_init(|| {
        let state = Arc::new(SharedSessionState::default());
        let hook_state = Arc::clone(&state);
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            hook_state.restore_best_effort();
            previous_hook(panic_info);
        }));
        state
    })
}

/// Guard that restores terminal modes and raw mode on both normal `Drop` and `panic!`.
#[derive(Debug)]
pub struct TerminalSessionGuard {
    state: Arc<SharedSessionState>,
}

impl TerminalSessionGuard {
    /// Installs the once-only panic hook and enables raw mode when `is_tty_available` is `true`.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if `crossterm::terminal::enable_raw_mode` fails on an active TTY.
    pub fn acquire(is_tty_available: bool) -> io::Result<Self> {
        let state = Arc::clone(shared_session_state());
        state.is_kitty_pushed.store(false, Ordering::Release);
        #[cfg(windows)]
        state
            .is_windows_mouse_captured
            .store(false, Ordering::Release);
        if is_tty_available {
            crossterm::terminal::enable_raw_mode()?;
            state.is_raw_mode_enabled.store(true, Ordering::Release);
            state.has_active_session.store(true, Ordering::Release);
        } else {
            state.is_raw_mode_enabled.store(false, Ordering::Release);
            state.has_active_session.store(false, Ordering::Release);
        }
        Ok(Self { state })
    }

    /// Updates whether Kitty keyboard enhancement flags are currently pushed on the terminal stack.
    pub fn set_kitty_pushed(&self, is_pushed: bool) {
        self.state
            .is_kitty_pushed
            .store(is_pushed, Ordering::Release);
    }

    /// Enables interactive mouse capture (rule M1 on Unix; console `EnableMouseCapture` on Windows).
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing the mouse setup sequence or updating console mode fails.
    #[cfg(unix)]
    pub fn enable_interactive_mouse(
        &self,
        writer: &mut TerminalWriter,
        log: &mut JsonLog,
    ) -> io::Result<()> {
        // Never call `crossterm::event::EnableMouseCapture` on Unix: it sends `?1003h` all-motion
        // tracking, violating rule M2. Write rule M1 (`?1000h?1002h?1006h`) directly instead.
        writer.write_logged("interactive:mouse_setup_m1", MOUSE_SETUP_M1, log)
    }

    /// Enables interactive mouse capture (rule M1 on Unix; console `EnableMouseCapture` on Windows).
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing the mouse setup sequence or updating console mode fails.
    #[cfg(windows)]
    pub fn enable_interactive_mouse(
        &self,
        writer: &mut TerminalWriter,
        log: &mut JsonLog,
    ) -> io::Result<()> {
        log.write_line(&format_write_record(
            "interactive:windows_enable_mouse_capture",
            b"ENABLE_MOUSE_INPUT",
        ))?;
        if writer.is_tty_available() {
            // On Windows, `EnableMouseCapture` sets console `ENABLE_MOUSE_INPUT` so ConPTY asks
            // the outer terminal for mouse tracking (M0 row 14).
            crossterm::execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;
            self.state
                .is_windows_mouse_captured
                .store(true, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for TerminalSessionGuard {
    fn drop(&mut self) {
        self.state.restore_best_effort();
    }
}

#[derive(Debug)]
pub(crate) enum OutputTarget {
    Detached,
    #[cfg(unix)]
    DevTty(File),
    Stdout,
}

/// Terminal output writer paired with JSONL write logging.
#[derive(Debug)]
pub struct TerminalWriter {
    pub(crate) target: OutputTarget,
}

impl TerminalWriter {
    /// Returns `true` when connected to an interactive TTY or console.
    #[must_use]
    pub const fn is_tty_available(&self) -> bool {
        !matches!(self.target, OutputTarget::Detached)
    }

    /// Logs `bytes` under `label` and writes/flushes them to the active terminal when present.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing to the JSONL log or terminal stream fails.
    pub fn write_logged(&mut self, label: &str, bytes: &[u8], log: &mut JsonLog) -> io::Result<()> {
        log.write_line(&format_write_record(label, bytes))?;
        match &mut self.target {
            OutputTarget::Detached => Ok(()),
            #[cfg(unix)]
            OutputTarget::DevTty(tty_file) => {
                tty_file.write_all(bytes)?;
                tty_file.flush()
            }
            OutputTarget::Stdout => {
                let mut stdout_handle = io::stdout();
                stdout_handle.write_all(bytes)?;
                stdout_handle.flush()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_guard_and_writer_leave_session_inactive_and_drop_cleanly() {
        let guard = TerminalSessionGuard::acquire(false).expect("detached guard must succeed");
        guard.set_kitty_pushed(true);
        assert!(!guard.state.has_active_session.load(Ordering::Acquire));
        drop(guard);

        let writer = TerminalWriter {
            target: OutputTarget::Detached,
        };
        assert!(!writer.is_tty_available());
    }
}
