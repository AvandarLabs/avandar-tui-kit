//! Terminal session guard (`Drop` + once-only panic hook) and logged terminal writer.

#[cfg(unix)]
use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::json_log::JsonLog;
use crate::section_json::format_write_record;
use crate::sequences::{
    BRACKETED_PASTE_OFF, FOCUS_EVENTS_OFF, KITTY_KEYBOARD_POP, MOUSE_TEARDOWN_M5, SYNC_OUTPUT_OFF,
};
#[cfg(unix)]
use crate::sequences::{
    BRACKETED_PASTE_ON, FOCUS_EVENTS_ON, KITTY_KEYBOARD_PUSH_DISAMBIGUATE, MOUSE_SETUP_M1,
};

/// Builds the full terminal-mode reset sequence for `Drop` and panic-hook teardown.
#[must_use]
pub fn build_teardown_bytes(was_kitty_pushed: bool) -> Vec<u8> {
    let mut teardown = Vec::with_capacity(64);
    teardown.extend_from_slice(MOUSE_TEARDOWN_M5);
    teardown.extend_from_slice(BRACKETED_PASTE_OFF);
    teardown.extend_from_slice(FOCUS_EVENTS_OFF);
    teardown.extend_from_slice(SYNC_OUTPUT_OFF);
    if was_kitty_pushed {
        teardown.extend_from_slice(KITTY_KEYBOARD_POP);
    }
    teardown
}

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
        let reset_bytes = build_teardown_bytes(was_kitty_pushed);
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
        if !is_tty_available {
            return Ok(Self {
                state: Arc::new(SharedSessionState::default()),
            });
        }
        let state = Arc::clone(shared_session_state());
        crossterm::terminal::enable_raw_mode()?;
        state.is_kitty_pushed.store(false, Ordering::Release);
        #[cfg(windows)]
        state
            .is_windows_mouse_captured
            .store(false, Ordering::Release);
        state.is_raw_mode_enabled.store(true, Ordering::Release);
        state.has_active_session.store(true, Ordering::Release);
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

    /// Enables platform-appropriate interactive input modes on the active terminal.
    ///
    /// On Unix, writes rule M1 mouse tracking, bracketed paste (`2004`), focus events (`1004`),
    /// and (when `is_push_kitty` is `true`) Kitty disambiguate mode (`>1u`).
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing mode sequences or logging fails.
    #[cfg(unix)]
    pub fn enable_interactive_modes(
        &self,
        writer: &mut TerminalWriter,
        is_push_kitty: bool,
        log: &mut JsonLog,
    ) -> io::Result<()> {
        self.enable_interactive_mouse(writer, log)?;
        writer.write_logged("interactive:bracketed_paste_on", BRACKETED_PASTE_ON, log)?;
        writer.write_logged("interactive:focus_events_on", FOCUS_EVENTS_ON, log)?;
        if is_push_kitty {
            self.set_kitty_pushed(true);
            writer.write_logged(
                "interactive:kitty_push",
                KITTY_KEYBOARD_PUSH_DISAMBIGUATE,
                log,
            )?;
        }
        Ok(())
    }

    /// Enables platform-appropriate interactive input modes on the active terminal.
    ///
    /// On Windows, enables console mouse capture via `crossterm` and leaves modes `2004`, `1004`,
    /// and Kitty flags off because `crossterm` reads console `INPUT_RECORD` events directly and
    /// does not parse bracketed paste escape sequences.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if updating console mouse mode or logging fails.
    #[cfg(windows)]
    pub fn enable_interactive_modes(
        &self,
        writer: &mut TerminalWriter,
        _is_push_kitty: bool,
        log: &mut JsonLog,
    ) -> io::Result<()> {
        self.enable_interactive_mouse(writer, log)
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
    fn detached_guard_and_writer_do_not_touch_shared_panic_hook_state() {
        let global_state = shared_session_state();
        global_state.is_kitty_pushed.store(true, Ordering::Release);

        let guard = TerminalSessionGuard::acquire(false).expect("detached guard must succeed");
        assert!(!Arc::ptr_eq(&guard.state, global_state));
        assert!(global_state.is_kitty_pushed.load(Ordering::Acquire));

        guard.set_kitty_pushed(false);
        assert!(global_state.is_kitty_pushed.load(Ordering::Acquire));
        assert!(!guard.state.has_active_session.load(Ordering::Acquire));
        drop(guard);

        global_state.is_kitty_pushed.store(false, Ordering::Release);
        let writer = TerminalWriter {
            target: OutputTarget::Detached,
        };
        assert!(!writer.is_tty_available());
    }

    #[cfg(unix)]
    #[test]
    fn enable_interactive_modes_writes_unix_sequences_and_tracks_kitty_push() {
        let base_path =
            std::env::temp_dir().join(format!("rig-probe-guard-unix-{}.jsonl", std::process::id()));
        let guard = TerminalSessionGuard::acquire(false).expect("detached guard must succeed");
        let mut writer = TerminalWriter {
            target: OutputTarget::Detached,
        };
        let mut log = JsonLog::create(&base_path).expect("log must create");
        guard
            .enable_interactive_modes(&mut writer, false, &mut log)
            .expect("unix interactive setup without kitty must succeed");
        assert!(!guard.state.is_kitty_pushed.load(Ordering::Acquire));

        guard
            .enable_interactive_modes(&mut writer, true, &mut log)
            .expect("unix interactive setup with kitty must succeed");
        assert!(guard.state.is_kitty_pushed.load(Ordering::Acquire));

        let logged = std::fs::read_to_string(&base_path).expect("log must be readable");
        let _ = std::fs::remove_file(&base_path);
        assert_eq!(logged.matches("interactive:mouse_setup_m1").count(), 2);
        assert_eq!(logged.matches("interactive:bracketed_paste_on").count(), 2);
        assert_eq!(logged.matches("interactive:focus_events_on").count(), 2);
        assert_eq!(logged.matches("interactive:kitty_push").count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn enable_interactive_modes_uses_windows_mouse_capture_and_skips_unix_modes() {
        let base_path =
            std::env::temp_dir().join(format!("rig-probe-guard-win-{}.jsonl", std::process::id()));
        let guard = TerminalSessionGuard::acquire(false).expect("detached guard must succeed");
        let mut writer = TerminalWriter {
            target: OutputTarget::Detached,
        };
        let mut log = JsonLog::create(&base_path).expect("log must create");
        guard
            .enable_interactive_modes(&mut writer, true, &mut log)
            .expect("windows interactive setup must succeed");
        assert!(!guard.state.is_kitty_pushed.load(Ordering::Acquire));

        let logged = std::fs::read_to_string(&base_path).expect("log must be readable");
        let _ = std::fs::remove_file(&base_path);
        assert!(logged.contains("interactive:windows_enable_mouse_capture"));
        assert!(!logged.contains("interactive:bracketed_paste_on"));
        assert!(!logged.contains("interactive:focus_events_on"));
        assert!(!logged.contains("interactive:kitty_push"));
    }
}
