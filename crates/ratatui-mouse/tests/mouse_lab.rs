//! Observable fixtures and protocol of the app-owned mouse laboratory.
#![cfg(feature = "vt100")]

#[path = "../examples/mouse_lab/app.rs"]
#[expect(
    dead_code,
    reason = "the interactive shell uses additional app operations"
)]
mod app;
#[path = "../examples/mouse_lab/args.rs"]
mod args;
#[path = "../examples/mouse_lab/log.rs"]
mod log;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui_mouse::{
    ContentPoint, ContentRange, MouseOutcome, MouseReport, MouseSettings, SelectableSurface,
};
use std::time::Duration;

#[test]
fn lab_fixture_and_log_records_are_deterministic() {
    let mut fixture = app::App::new(MouseSettings::default());
    fixture.set_layout(Rect::new(0, 0, 80, 24));
    assert_fixed_surfaces(&mut fixture);
    assert_log_protocol(&fixture);
}

fn assert_fixed_surfaces(fixture: &mut app::App) {
    assert_eq!(
        &fixture.lines[..4],
        &["alpha", "beta", "gamma", "file.rs:12"]
    );
    let mut left = fixture.left_surface();
    assert_eq!(
        left.text(ContentRange::new(
            ContentPoint { line: 0, column: 0 },
            ContentPoint { line: 2, column: 4 }
        )),
        "alpha\nbeta\ngamma"
    );
    assert_eq!(
        {
            let range = left.line_at(5);
            left.text(range)
        },
        "soft wrap over three rows"
    );
    assert!(*left.line_bounds().end() > 24);
    let mut right = fixture.right_surface();
    assert!(*right.line_bounds().end() > 24);
    assert!(
        {
            let range = right.line_at(0);
            right.text(range)
        }
        .contains("history")
    );
}

fn assert_log_protocol(fixture: &app::App) {
    let text = "a \"quote\"\nand 界";
    let report = MouseReport {
        outcome: MouseOutcome::Copy {
            target: app::Target::Left,
            text: text.to_owned(),
        },
        pressed_target: Some(app::Target::Left),
        is_redraw_due: true,
    };
    let mut bytes = Vec::new();
    let mut writer = log::Log::new(&mut bytes);
    writer.record(log::report_record(&report)).unwrap();
    writer.record(log::layout_record(&fixture.areas)).unwrap();
    writer
        .record(log::report_record(&MouseReport {
            outcome: MouseOutcome::Forward {
                target: app::Target::Right,
                bytes: vec![27, 91, 77],
            },
            pressed_target: None,
            is_redraw_due: false,
        }))
        .unwrap();
    let records: Vec<serde_json::Value> = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_report_rows(&records, text);
}

fn assert_report_rows(records: &[serde_json::Value], text: &str) {
    assert_eq!(records[0]["outcome"]["text"], text);
    assert_eq!(records[0]["outcome"]["kind"], "Copy");
    assert_eq!(records[0]["pressed_target"], "left");
    assert_eq!(records[1]["areas"][0]["target"], "left");
    assert_eq!(
        records[2]["outcome"]["bytes"],
        serde_json::json!([27, 91, 77])
    );
    assert_eq!(
        records
            .iter()
            .map(|record| record["sequence"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn settings_override_defaults_and_reject_invalid_values() {
    let settings = args::parse_settings(
        r#"{"auto_scroll_interval_ms":75,"wheel_step_rows":7,"word_characters":"!"}"#,
    )
    .unwrap();
    assert_eq!(settings.auto_scroll_interval, Duration::from_millis(75));
    assert_eq!(settings.wheel_step_rows, 7);
    assert!(settings.word_characters.contains('!'));
    assert_eq!(
        settings.redraw_interval,
        MouseSettings::default().redraw_interval
    );
    for invalid in [
        r#"{"wat":true}"#,
        r#"{"wheel_step_rows":-1}"#,
        r#"{"auto_scroll_interval_ms":0}"#,
        r#"{"redraw_interval_ms":1.5}"#,
        r#"{"wheel_step_rows":null}"#,
        "[]",
        "null",
    ] {
        assert!(
            args::parse_settings(invalid)
                .unwrap_err()
                .contains("--help")
        );
    }
}

#[test]
fn help_precedes_invalid_settings_and_arguments() {
    assert!(
        args::parse_args(["--settings", "bad", "--unknown", "--help"])
            .unwrap()
            .is_none()
    );
    let args = args::parse_args(["--log", "a path.jsonl", "--query-modes"])
        .unwrap()
        .unwrap();
    assert_eq!(args.log_path.to_str(), Some("a path.jsonl"));
    assert!(args.should_query_modes);
    for invalid in [vec!["--log"], vec!["--unknown"], vec!["--settings", "bad"]] {
        let error = args::parse_args(invalid).unwrap_err();
        assert!(error.contains("⚠️"));
        assert!(error.contains("Usage:"));
    }
}

#[test]
fn app_keys_yield_to_selection_and_mouse_reports_drive_the_fixture() {
    let mut app = app::App::new(MouseSettings::default());
    app.set_layout(Rect::new(0, 0, 80, 24));
    let now = std::time::Instant::now();
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let _ = app.handle_event(&key(KeyCode::Tab), now);
    assert_eq!(app.focused, app::Target::Right);
    let _ = app.handle_event(&key(KeyCode::Char('m')), now);
    assert_eq!(
        app.parser.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::PressRelease
    );
    let _ = app.handle_event(&key(KeyCode::Char('a')), now);
    assert!(app.parser.screen().alternate_screen());
    let _ = app.handle_event(&key(KeyCode::Tab), now);
    assert_keyboard_path_copy(&mut app, now);
    let _ = app.handle_event(&key(KeyCode::Char('s')), now);
    let _ = app.handle_event(&key(KeyCode::Char('!')), now);
    let _ = app.handle_event(&key(KeyCode::Char('q')), now);
    assert!(!app.should_quit);
    app.controller.exit_select_mode();
    let area = app.areas.area_of(&app::Target::Left).unwrap();
    let _ = app.handle_event(
        &Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: area.x,
            row: area.y,
            modifiers: KeyModifiers::NONE,
        }),
        now,
    );
    assert_eq!(app.scroll.line, 3);
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x,
        row: area.y,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(
        log::event_record(&event)["event"]["mouse_kind"],
        "Down(Left)"
    );
    let _ = app.handle_event(&key(KeyCode::Char('q')), now);
    assert!(app.should_quit);
}

fn assert_keyboard_path_copy(app: &mut app::App, now: std::time::Instant) {
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let _ = app.handle_event(&key(KeyCode::Char('s')), now);
    for code in [
        KeyCode::Char('g'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('j'),
        KeyCode::Char('W'),
    ] {
        let _ = app.handle_event(&key(code), now);
    }
    let selection = log::selection_record(app.controller.selection());
    assert_eq!(selection["selection"]["range"]["start"]["line"], 3);
    assert_eq!(selection["selection"]["unit"], "Word");
    let report = app.handle_event(&key(KeyCode::Char('y')), now);
    assert_eq!(
        report.outcome,
        MouseOutcome::Copy {
            target: app::Target::Left,
            text: "file.rs:12".into()
        }
    );
}

use session::query;

#[test]
fn mode_reply_parser_handles_partial_noise_and_absence() {
    let replies = query::parse_mode_replies(b"noise\x1b[?1002;1$y\x1b[?1006;2$y\x1b[?1003;");
    assert_eq!(replies, vec![(1002, 1), (1006, 2)]);
    assert_eq!(
        query::parse_mode_replies(b"no replies"),
        Vec::<(u16, u8)>::new()
    );
}

#[cfg(unix)]
#[test]
fn absent_mode_replies_leave_no_reader_stealing_later_input() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    assert_eq!(
        query::collect_reply(&mut reader, Duration::from_millis(1)).unwrap(),
        Vec::<u8>::new()
    );
    writer.write_all(b"q").unwrap();
    let mut byte = [0];
    reader.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [b'q']);
    writer.write_all(b"\x1b[?1002;1$y\x1b[?1006;1$y").unwrap();
    let bytes = query::collect_reply(&mut reader, Duration::from_millis(1)).unwrap();
    assert_eq!(
        query::parse_mode_replies(&bytes),
        vec![(1002, 1), (1006, 1)]
    );
}

#[path = "../examples/mouse_lab/session.rs"]
#[expect(
    dead_code,
    reason = "the shell owns session acquisition and normal cleanup"
)]
mod session;

#[cfg(unix)]
#[test]
fn cleanup_restores_mouse_before_screen_and_continues_after_write_error() {
    struct FailOnce {
        bytes: Vec<u8>,
        has_failed: bool,
    }
    impl std::io::Write for FailOnce {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self.has_failed {
                self.has_failed = true;
                return Err(std::io::Error::other("failed mouse reset"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bytes = Vec::new();
    session::restore_output(&mut bytes, true).unwrap();
    assert!(bytes.starts_with(ratatui_mouse::DISABLE_MOUSE_REPORTING));
    assert!(bytes.ends_with(b"\x1b[?1049l\x1b[?25h"));
    let mut writer = FailOnce {
        bytes: vec![],
        has_failed: false,
    };
    assert!(session::restore_output(&mut writer, true).is_err());
    assert!(writer.bytes.ends_with(b"\x1b[?1049l\x1b[?25h"));
}

#[test]
fn readiness_marker_requires_a_flushed_ready_record() {
    let marker = std::env::temp_dir().join(format!("mouse-lab-ready-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let mut bytes = Vec::new();
    let mut log = log::Log::new(&mut bytes);
    log.ready(&marker).unwrap();
    assert_eq!(std::fs::read(&marker).unwrap(), b"ready\n");
    let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(record["kind"], "ready");
    assert_eq!(record["sequence"], 1);
    std::fs::remove_file(&marker).unwrap();
    let mut log = log::Log::new(FlushFailure);
    assert!(log.ready(&marker).is_err());
    assert!(!marker.exists());
}

struct FlushFailure;
impl std::io::Write for FlushFailure {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("log flush failed"))
    }
}

#[test]
fn pointer_focus_change_requests_redraw_even_when_forwarded() {
    let mut app = app::App::new(MouseSettings::default());
    app.set_layout(Rect::new(0, 0, 80, 24));
    let now = std::time::Instant::now();
    let _ = app.handle_event(
        &Event::Key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)),
        now,
    );
    let area = app.areas.area_of(&app::Target::Right).unwrap();
    let event = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x,
        row: area.y,
        modifiers: KeyModifiers::NONE,
    });
    let report = app.handle_event(&event, now);
    assert_eq!(app.focused, app::Target::Right);
    assert!(matches!(report.outcome, MouseOutcome::Forward { .. }));
    assert!(report.is_redraw_due);
}
