//! Terminal ownership and panic cleanup for this example only.
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[cfg(unix)]
use std::time::Duration;

use super::log::Log;
use ratatui_mouse::{disable_mouse_reporting, enable_mouse_reporting};
use serde_json::{Value, json};

#[path = "session/query.rs"]
pub mod query;

const ENTER_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l";
const LEAVE_SCREEN: &[u8] = b"\x1b[?1049l\x1b[?25h";

pub struct Session {
    state: Arc<State>,
    ready_path: PathBuf,
}

struct State {
    is_active: AtomicBool,
    is_raw: AtomicBool,
    is_alternate: AtomicBool,
    log: Mutex<Log<File>>,
}

impl State {
    fn restore(&self, after_modes: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        if !self.is_active.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let output_result = restore_output(
            &mut io::stdout(),
            self.is_alternate.swap(false, Ordering::AcqRel),
        );
        // Cleanup never waits on a lock potentially held by the panicking thread.
        if let Ok(mut log) = self.log.try_lock() {
            let _ = log.record(sequence_record("teardown", teardown_bytes()));
            let _ = log.record(sequence_record("leave_screen", LEAVE_SCREEN));
        }
        let query_result = after_modes();
        let raw_result = if self.is_raw.swap(false, Ordering::AcqRel) {
            crossterm::terminal::disable_raw_mode()
        } else {
            Ok(())
        };
        output_result.and(query_result).and(raw_result)
    }
}

impl Session {
    pub fn start(path: &Path) -> io::Result<Self> {
        let mut ready_name = path.as_os_str().to_os_string();
        ready_name.push(".ready");
        let ready_path = PathBuf::from(ready_name);
        match fs::remove_file(&ready_path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let state = Arc::new(State {
            is_active: AtomicBool::new(true),
            is_raw: AtomicBool::new(false),
            is_alternate: AtomicBool::new(false),
            log: Mutex::new(Log::new(File::create(path)?)),
        });
        let session = Self {
            state: Arc::clone(&state),
            ready_path,
        };
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = state.restore(|| Ok(()));
            previous(info);
        }));
        crossterm::terminal::enable_raw_mode()?;
        session.state.is_raw.store(true, Ordering::Release);
        session.state.is_alternate.store(true, Ordering::Release);
        session.write_sequence("enter_screen", ENTER_SCREEN)?;
        enable_mouse_reporting(&mut io::stdout()).map_err(io::Error::other)?;
        session.record(sequence_record("setup", setup_bytes()))?;
        Ok(session)
    }

    pub fn record(&self, value: Value) -> io::Result<()> {
        self.state
            .log
            .lock()
            .map_err(|_| io::Error::other("mouse_lab log lock poisoned"))?
            .record(value)
    }

    pub fn ready(&self) -> io::Result<()> {
        self.state
            .log
            .lock()
            .map_err(|_| io::Error::other("mouse_lab log lock poisoned"))?
            .ready(&self.ready_path)
    }

    pub fn finish(&self, should_query_modes: bool) -> io::Result<()> {
        self.state.restore(|| {
            if should_query_modes {
                self.query_modes("teardown")
            } else {
                Ok(())
            }
        })
    }

    fn write_sequence(&self, label: &str, bytes: &[u8]) -> io::Result<()> {
        let mut stdout = io::stdout();
        stdout.write_all(bytes)?;
        stdout.flush()?;
        self.record(sequence_record(label, bytes))
    }

    pub fn query_modes(&self, phase: &str) -> io::Result<()> {
        let mut query = String::new();
        for mode in query::MODES {
            use std::fmt::Write as _;
            write!(query, "\x1b[?{mode}$p").expect("formatting into a String cannot fail");
        }
        self.write_sequence(&format!("query_{phase}"), query.as_bytes())?;
        #[cfg(unix)]
        let bytes = query::read_terminal_replies(Duration::from_millis(250))?;
        #[cfg(not(unix))]
        let bytes: Vec<u8> = Vec::new();
        let replies = query::parse_mode_replies(&bytes);
        for mode in query::MODES {
            let state = replies
                .iter()
                .find(|(replied, _)| *replied == mode)
                .map(|(_, state)| *state);
            self.record(json!({"kind":"mode_reply","phase":phase,"mode":mode,"state":state,"is_absent":state.is_none()}))?;
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.state.restore(|| Ok(()));
    }
}

pub fn restore_output<W: Write>(writer: &mut W, is_alternate: bool) -> io::Result<()> {
    let mouse_result = disable_mouse_reporting(writer).map_err(io::Error::other);
    let screen_result = if is_alternate {
        writer.write_all(LEAVE_SCREEN).and_then(|()| writer.flush())
    } else {
        Ok(())
    };
    mouse_result.and(screen_result)
}

fn sequence_record(label: &str, bytes: &[u8]) -> Value {
    json!({"kind":"sequence","label":label,"bytes":bytes})
}

#[cfg(unix)]
const fn setup_bytes() -> &'static [u8] {
    ratatui_mouse::ENABLE_MOUSE_REPORTING
}
#[cfg(not(unix))]
const fn setup_bytes() -> &'static [u8] {
    b""
}
#[cfg(unix)]
const fn teardown_bytes() -> &'static [u8] {
    ratatui_mouse::DISABLE_MOUSE_REPORTING
}
#[cfg(not(unix))]
const fn teardown_bytes() -> &'static [u8] {
    b""
}
