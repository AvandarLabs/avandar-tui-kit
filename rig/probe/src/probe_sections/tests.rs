use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::terminal_io::DemandByteReader;

static NEXT_SECTION_TEST_ID: AtomicUsize = AtomicUsize::new(1);

struct ChunkSequenceReader {
    chunks: VecDeque<Vec<u8>>,
}

impl Read for ChunkSequenceReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some(chunk) = self.chunks.pop_front() else {
            return Ok(0);
        };
        buffer[..chunk.len()].copy_from_slice(&chunk);
        Ok(chunk.len())
    }
}

fn temp_log_path() -> PathBuf {
    let id = NEXT_SECTION_TEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "rig-probe-sections-{}-{id}.jsonl",
        std::process::id()
    ))
}

#[test]
fn kitty_keyboard_pops_stack_even_when_pushed_query_fence_times_out() {
    let log_path = temp_log_path();
    let mut log = JsonLog::create(&log_path).expect("log must create");
    let reader = DemandByteReader::spawn(ChunkSequenceReader {
        // Answers initial_query (`?0u` + DA1), then EOF on pushed_query to trigger timeout.
        chunks: VecDeque::from([b"\x1b[?0u\x1b[?6c".to_vec()]),
    });
    let mut session = RawQuerySession::for_test(reader);
    let guard = TerminalSessionGuard::acquire(false).expect("detached guard");

    run_kitty_keyboard_section(&mut session, &guard, &mut log)
        .expect("kitty section must recover cleanly");
    let logged = fs::read_to_string(&log_path).expect("log must be readable");
    let _ = fs::remove_file(&log_path);

    assert!(logged.contains("\"hex\":\"1b5b3e3175"));
    assert!(logged.contains("\"hex\":\"1b5b3c75"));
}

#[test]
fn modes_section_disables_toggled_mode_when_enable_fence_times_out() {
    let log_path = temp_log_path();
    let mut log = JsonLog::create(&log_path).expect("log must create");
    let reader = DemandByteReader::spawn(ChunkSequenceReader {
        chunks: VecDeque::new(),
    });
    let mut session = RawQuerySession::for_test(reader);

    run_modes_section(&mut session, &mut log).expect("modes section must recover cleanly");
    let logged = fs::read_to_string(&log_path).expect("log must be readable");
    let _ = fs::remove_file(&log_path);

    assert!(logged.contains("1b5b3f3230303468"));
    assert!(logged.contains("1b5b3f323030346c"));
    assert!(!logged.contains("1b5b3f3130303468"));
}

#[test]
fn mouse_modes_skips_setup_when_session_is_detached_or_initial_fence_times_out() {
    let non_raw_path = temp_log_path();
    let mut non_raw_log = JsonLog::create(&non_raw_path).expect("log must create");
    let mut non_raw_session = RawQuerySession::detached_for_test();
    run_mouse_modes_section(&mut non_raw_session, &mut non_raw_log)
        .expect("detached mouse section must succeed");
    let non_raw_logged = fs::read_to_string(&non_raw_path).expect("log must be readable");
    let _ = fs::remove_file(&non_raw_path);
    assert!(!non_raw_logged.contains("mouse_modes:setup_m1"));

    let initial_timeout_path = temp_log_path();
    let mut initial_timeout_log = JsonLog::create(&initial_timeout_path).expect("log must create");
    let empty_reader = DemandByteReader::spawn(ChunkSequenceReader {
        chunks: VecDeque::new(),
    });
    let mut initial_timeout_session = RawQuerySession::for_test(empty_reader);
    run_mouse_modes_section(&mut initial_timeout_session, &mut initial_timeout_log)
        .expect("initial-timeout mouse section must succeed");
    let initial_timeout_logged =
        fs::read_to_string(&initial_timeout_path).expect("log must be readable");
    let _ = fs::remove_file(&initial_timeout_path);
    assert!(!initial_timeout_logged.contains("mouse_modes:setup_m1"));
}

#[test]
fn mouse_modes_emits_m5_teardown_even_when_after_m1_fence_times_out() {
    let timeout_path = temp_log_path();
    let mut timeout_log = JsonLog::create(&timeout_path).expect("log must create");
    let reader = DemandByteReader::spawn(ChunkSequenceReader {
        chunks: VecDeque::from([b"\x1b[?1002;2$y\x1b[?6c".to_vec()]),
    });
    let mut timeout_session = RawQuerySession::for_test(reader);
    run_mouse_modes_section(&mut timeout_session, &mut timeout_log)
        .expect("timed-out mouse section must recover cleanly");
    let timeout_logged = fs::read_to_string(&timeout_path).expect("log must be readable");
    let _ = fs::remove_file(&timeout_path);
    assert!(timeout_logged.contains("mouse_modes:setup_m1"));
    assert!(timeout_logged.contains("mouse_modes:teardown_m5"));
}

fn run_osc52_case(token: Option<&str>, form: Osc52Form, is_tmux: bool) -> String {
    let log_path = temp_log_path();
    let mut log = JsonLog::create(&log_path).expect("log must create");
    let mut session = RawQuerySession::detached_for_test();
    let args = ProbeArgs {
        log_path: log_path.clone(),
        is_batch: true,
        is_push_kitty: false,
        osc52_token: token.map(str::to_owned),
        osc52_form: form,
    };
    run_osc52_section(&mut session, &args, is_tmux, &mut log).expect("osc52 section must succeed");
    let logged = fs::read_to_string(&log_path).expect("log must be readable");
    let _ = fs::remove_file(&log_path);
    logged
}

#[test]
fn osc52_section_respects_osc52_form_across_plain_and_tmux_environments() {
    let unconfigured = run_osc52_case(None, Osc52Form::Both, true);
    assert!(unconfigured.contains(r#""emitted":false,"plain_sent":false,"tmux_sent":false"#));
    assert!(!unconfigured.contains("osc52:plain") && !unconfigured.contains("osc52:tmux"));

    let cases = [
        (Osc52Form::Auto, false, true, false),
        (Osc52Form::Auto, true, true, true),
        (Osc52Form::Plain, true, true, false),
        (Osc52Form::Tmux, false, false, true),
        (Osc52Form::Both, false, true, true),
    ];
    for (form, is_tmux, expect_plain, expect_tmux) in cases {
        let logged = run_osc52_case(Some("tok"), form, is_tmux);
        let expected_section =
            format!(r#""emitted":true,"plain_sent":{expect_plain},"tmux_sent":{expect_tmux}"#);
        assert!(logged.contains(&expected_section), "logged: {logged}");
        assert_eq!(logged.contains("osc52:plain"), expect_plain);
        assert_eq!(logged.contains("osc52:tmux"), expect_tmux);
    }
}
