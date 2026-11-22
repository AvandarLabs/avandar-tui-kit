//! Reporting bytes and lifecycle behavior on stream-based terminals.
#![cfg(not(windows))]

use std::{
    cell::RefCell,
    error::Error,
    io::{self, Write},
    rc::Rc,
};

use ratatui_mouse::{
    DISABLE_MOUSE_REPORTING, ENABLE_MOUSE_REPORTING, MouseError, MouseReporting,
    disable_mouse_reporting, enable_mouse_reporting,
};

#[derive(Default)]
struct Recording {
    bytes: Vec<u8>,
    flushes: usize,
    max_chunk: Option<usize>,
    fail_write_after: Option<usize>,
    fail_flush_on: Option<usize>,
}

#[derive(Clone, Default)]
struct RecordingWriter(Rc<RefCell<Recording>>);

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut recording = self.0.borrow_mut();
        if recording.fail_write_after == Some(recording.bytes.len()) {
            recording.fail_write_after = None;
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "injected write failure",
            ));
        }
        let remaining = recording
            .fail_write_after
            .map_or(usize::MAX, |limit| limit - recording.bytes.len());
        let written = buffer
            .len()
            .min(recording.max_chunk.unwrap_or(usize::MAX))
            .min(remaining);
        recording.bytes.extend_from_slice(&buffer[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut recording = self.0.borrow_mut();
        recording.flushes += 1;
        if recording.fail_flush_on == Some(recording.flushes) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "injected flush failure",
            ));
        }
        Ok(())
    }
}

#[test]
fn modes_bytes_and_guard_restore_once() {
    assert_eq!(ENABLE_MOUSE_REPORTING, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
    assert_eq!(
        DISABLE_MOUSE_REPORTING,
        b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l"
    );
    for explicit_disable in [false, true] {
        let writer = RecordingWriter::default();
        let guard = MouseReporting::enable(writer.clone()).unwrap();
        assert_eq!(writer.0.borrow().bytes, ENABLE_MOUSE_REPORTING);
        assert_eq!(writer.0.borrow().flushes, 1);
        if explicit_disable {
            guard.disable().unwrap();
        } else {
            drop(guard);
        }
        let recording = writer.0.borrow();
        assert_eq!(
            recording.bytes,
            [ENABLE_MOUSE_REPORTING, DISABLE_MOUSE_REPORTING].concat()
        );
        assert_eq!(recording.flushes, 2);
    }
    let mut writer = RecordingWriter::default();
    disable_mouse_reporting(&mut writer).unwrap();
    disable_mouse_reporting(&mut writer).unwrap();
    assert_eq!(writer.0.borrow().bytes, DISABLE_MOUSE_REPORTING.repeat(2));
    assert_eq!(writer.0.borrow().flushes, 2);
}

#[test]
fn write_and_flush_failures_preserve_io_source() {
    let mut writer = RecordingWriter::default();
    writer.0.borrow_mut().max_chunk = Some(2);
    enable_mouse_reporting(&mut writer).unwrap();
    disable_mouse_reporting(&mut writer).unwrap();
    assert_eq!(
        writer.0.borrow().bytes,
        [ENABLE_MOUSE_REPORTING, DISABLE_MOUSE_REPORTING].concat()
    );
    assert_eq!(writer.0.borrow().flushes, 2);
    check_standalone_errors();
    check_failed_enable_cleanup();
    check_failed_explicit_cleanup();
    check_drop_ignores_cleanup_failure();
}

fn assert_io_source(error: &MouseError, is_enable: bool, is_flush: bool) {
    assert!(matches!(
        (is_enable, error),
        (true, MouseError::EnableReporting(_)) | (false, MouseError::DisableReporting(_))
    ));
    let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
    let (kind, message) = if is_flush {
        (io::ErrorKind::TimedOut, "injected flush failure")
    } else {
        (io::ErrorKind::BrokenPipe, "injected write failure")
    };
    assert_eq!(source.kind(), kind);
    assert_eq!(source.to_string(), message);
    assert!(error.to_string().contains(message));
}

fn check_standalone_errors() {
    for is_enable in [false, true] {
        for is_flush in [false, true] {
            let mut writer = RecordingWriter::default();
            if is_flush {
                writer.0.borrow_mut().fail_flush_on = Some(1);
            } else {
                writer.0.borrow_mut().fail_write_after = Some(0);
            }
            let error = if is_enable {
                enable_mouse_reporting(&mut writer).unwrap_err()
            } else {
                disable_mouse_reporting(&mut writer).unwrap_err()
            };
            assert_io_source(&error, is_enable, is_flush);
        }
    }
}

fn check_failed_enable_cleanup() {
    for use_guard in [false, true] {
        for is_flush in [false, true] {
            let mut writer = RecordingWriter::default();
            writer.0.borrow_mut().fail_flush_on = Some(1);
            if !is_flush {
                // Cleanup itself fails, but the original write error must survive.
                writer.0.borrow_mut().fail_write_after = Some(7);
            }
            let error = if use_guard {
                match MouseReporting::enable(writer.clone()) {
                    Ok(_) => panic!("enable unexpectedly succeeded"),
                    Err(error) => error,
                }
            } else {
                enable_mouse_reporting(&mut writer).unwrap_err()
            };
            assert_io_source(&error, true, is_flush);
            let prefix = if is_flush {
                ENABLE_MOUSE_REPORTING
            } else {
                &ENABLE_MOUSE_REPORTING[..7]
            };
            let recording = writer.0.borrow();
            assert_eq!(recording.bytes, [prefix, DISABLE_MOUSE_REPORTING].concat());
            assert_eq!(recording.flushes, if is_flush { 2 } else { 1 });
        }
    }
}

fn check_failed_explicit_cleanup() {
    for is_flush in [false, true] {
        let writer = RecordingWriter::default();
        let guard = MouseReporting::enable(writer.clone()).unwrap();
        if is_flush {
            writer.0.borrow_mut().fail_flush_on = Some(2);
        } else {
            writer.0.borrow_mut().fail_write_after = Some(ENABLE_MOUSE_REPORTING.len());
        }
        let error = guard.disable().unwrap_err();
        assert_io_source(&error, false, is_flush);
        let teardown_count = if is_flush { 2 } else { 1 };
        let recording = writer.0.borrow();
        assert_eq!(
            recording.bytes,
            [
                ENABLE_MOUSE_REPORTING,
                &DISABLE_MOUSE_REPORTING.repeat(teardown_count)
            ]
            .concat()
        );
        assert_eq!(recording.flushes, 1 + teardown_count);
    }
}

fn check_drop_ignores_cleanup_failure() {
    for is_flush in [false, true] {
        let writer = RecordingWriter::default();
        let guard = MouseReporting::enable(writer.clone()).unwrap();
        if is_flush {
            writer.0.borrow_mut().fail_flush_on = Some(2);
        } else {
            writer.0.borrow_mut().fail_write_after = Some(ENABLE_MOUSE_REPORTING.len());
        }
        drop(guard);
        let recording = writer.0.borrow();
        let expected = if is_flush {
            [ENABLE_MOUSE_REPORTING, DISABLE_MOUSE_REPORTING].concat()
        } else {
            ENABLE_MOUSE_REPORTING.to_vec()
        };
        assert_eq!(recording.bytes, expected);
        assert_eq!(recording.flushes, if is_flush { 2 } else { 1 });
    }
}
