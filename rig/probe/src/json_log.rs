//! Zero-dependency JSON string escaping, JSONL line logging, and `.ready` sentinel files.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const LOWERCASE_HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Escapes `input` for safe inclusion inside a double-quoted JSON string literal.
#[must_use]
pub fn escape_json_string(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\x08' => escaped.push_str("\\b"),
            '\x0c' => escaped.push_str("\\f"),
            control_char if control_char < '\x20' => {
                if let Ok(control_byte) = u8::try_from(control_char) {
                    let high_nibble = usize::from(control_byte >> 4);
                    let low_nibble = usize::from(control_byte & 0x0f);
                    escaped.push_str("\\u00");
                    escaped.push(char::from(LOWERCASE_HEX_DIGITS[high_nibble]));
                    escaped.push(char::from(LOWERCASE_HEX_DIGITS[low_nibble]));
                }
            }
            plain_char => escaped.push(plain_char),
        }
    }
    escaped
}

/// Returns the `.ready` sentinel path paired with `log_path` (`<log_path>.ready`).
#[must_use]
pub fn ready_file_path(log_path: &Path) -> PathBuf {
    let mut sentinel_os_string = log_path.as_os_str().to_os_string();
    sentinel_os_string.push(".ready");
    PathBuf::from(sentinel_os_string)
}

/// Flushed JSON Lines writer that manages its paired `.ready` sentinel file.
#[derive(Debug)]
pub struct JsonLog {
    file: File,
    ready_path: PathBuf,
}

impl JsonLog {
    /// Creates or truncates `log_path`, creating parent directories and clearing any stale `.ready` file.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if parent directories cannot be created, if removing a pre-existing
    /// `.ready` file fails for a reason other than [`io::ErrorKind::NotFound`], or if `log_path`
    /// cannot be created.
    pub fn create(log_path: &Path) -> io::Result<Self> {
        if let Some(parent_dir) = log_path.parent()
            && !parent_dir.as_os_str().is_empty()
        {
            fs::create_dir_all(parent_dir)?;
        }
        let ready_path = ready_file_path(log_path);
        match fs::remove_file(&ready_path) {
            Ok(()) => {}
            Err(remove_error) if remove_error.kind() == io::ErrorKind::NotFound => {}
            Err(remove_error) => return Err(remove_error),
        }
        let file = File::create(log_path)?;
        Ok(Self { file, ready_path })
    }

    /// Appends `json_object_line` followed by `\n` and flushes the underlying file handle.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing to or flushing the log file fails.
    pub fn write_line(&mut self, json_object_line: &str) -> io::Result<()> {
        self.file.write_all(json_object_line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()
    }

    /// Appends `{"kind":"ready"}` to the log and creates the `<log_path>.ready` sentinel file.
    ///
    /// # Errors
    ///
    /// Returns an [`io::Error`] if writing the ready record or creating the `.ready` file fails.
    pub fn mark_ready(&mut self) -> io::Result<()> {
        self.write_line("{\"kind\":\"ready\"}")?;
        fs::write(&self.ready_path, b"ready\n")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(1);

    fn unique_temp_dir() -> PathBuf {
        let test_id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "rig-probe-json-log-{}-{test_id}",
            std::process::id()
        ))
    }

    #[test]
    fn escapes_quotes_backslashes_named_escapes_and_low_control_bytes() {
        assert_eq!(escape_json_string("a\"b\\c\n\x1b"), "a\\\"b\\\\c\\n\\u001b");
        assert_eq!(
            escape_json_string("\r\t\x08\x0c\x00\x1f"),
            "\\r\\t\\b\\f\\u0000\\u001f"
        );
        assert_eq!(escape_json_string("plain-\u{6F22}"), "plain-\u{6F22}");
    }

    #[test]
    fn builds_ready_sentinel_path_preserving_existing_extension() {
        assert_eq!(
            ready_file_path(Path::new("/tmp/probe.jsonl")),
            PathBuf::from("/tmp/probe.jsonl.ready")
        );
        assert_eq!(
            ready_file_path(Path::new("probe")),
            PathBuf::from("probe.ready")
        );
    }

    #[test]
    fn creates_parent_dirs_clears_stale_ready_and_writes_ready_marker() {
        let root_dir = unique_temp_dir();
        let nested_log_path = root_dir.join("nested").join("probe.jsonl");
        let sentinel_path = ready_file_path(&nested_log_path);

        let mut first_log = JsonLog::create(&nested_log_path).expect("should create log");
        assert!(!sentinel_path.exists());
        first_log
            .write_line("{\"kind\":\"section\"}")
            .expect("should write section line");
        first_log.mark_ready().expect("should mark ready");
        assert!(sentinel_path.exists());

        let contents = fs::read_to_string(&nested_log_path).expect("should read log");
        assert_eq!(contents, "{\"kind\":\"section\"}\n{\"kind\":\"ready\"}\n");

        let _reopened_log = JsonLog::create(&nested_log_path).expect("should recreate log");
        assert!(!sentinel_path.exists());
        assert_eq!(
            fs::read_to_string(&nested_log_path).expect("should read truncated log"),
            ""
        );

        fs::remove_dir_all(&root_dir).expect("should clean up temp dir");
    }
}
