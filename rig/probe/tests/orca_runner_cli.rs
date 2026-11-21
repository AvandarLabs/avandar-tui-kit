//! Integration tests for the Layer 5 unfocused Orca runner (`rig/orca/run.sh`).

use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rig/probe must sit two levels below workspace root")
        .to_path_buf()
}

fn runner_script_path() -> PathBuf {
    repository_root().join("rig/orca/run.sh")
}

#[cfg(windows)]
mod windows_tests {
    use std::fs;

    use super::runner_script_path;

    #[test]
    fn runner_script_exists_and_never_requests_focus() {
        let script_contents = fs::read_to_string(runner_script_path())
            .expect("rig/orca/run.sh must be readable from repository root");
        assert!(script_contents.starts_with("#!/usr/bin/env bash"));
        assert!(!script_contents.contains("--focus"));
        assert!(!script_contents.contains('\u{2014}'));
    }
}

#[cfg(unix)]
mod unix_tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{repository_root, runner_script_path};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    const MOCK_ORCA_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
if [ -n "${MOCK_ORCA_CALLS_FILE:-}" ]; then
  { printf 'CALL'; for item in "$@"; do printf '\t%s' "$item"; done; printf '\n'; } >> "$MOCK_ORCA_CALLS_FILE"
fi
if [ "${1:-}" = "terminal" ] && [ "${2:-}" = "create" ]; then
  shift 2; command_text=""
  while [ "$#" -gt 0 ]; do
    case "$1" in --command) command_text="${2:-}"; shift 2 ;; *) shift ;; esac
  done
  if [ "${MOCK_ORCA_SKIP_EXEC:-0}" != "1" ] && [ -n "$command_text" ]; then
    ( /bin/sh -c "$command_text" </dev/null >/dev/null 2>&1 ) || true
  fi
  printf '{"id":"req-create","ok":true,"result":{"terminal":{"handle":"term_mock_123","surface":"tab"}}}\n'
  exit 0
fi
if [ "${1:-}" = "terminal" ] && [ "${2:-}" = "wait" ]; then
  printf '{"id":"req-wait","ok":true,"result":{"wait":{"handle":"term_mock_123","condition":"exit","satisfied":true,"status":"exited","exitCode":0}}}\n'
  exit 0
fi
if [ "${1:-}" = "terminal" ] && [ "${2:-}" = "close" ]; then
  printf '{"id":"req-close","ok":true,"result":{"closed":true}}\n'
  exit 0
fi
exit 1
"#;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn create(label: &str) -> Self {
            let unique_id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir_name = format!(
                "tuikit-orca-cli-test-{label}-{}-{unique_id}",
                std::process::id()
            );
            let path = env::temp_dir().join(dir_name);
            fs::create_dir_all(&path).expect("temp test dir must be creatable");
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_mock_orca_binary(bin_directory: &Path) -> String {
        fs::create_dir_all(bin_directory).expect("mock bin dir must be creatable");
        let binary_path = bin_directory.join("orca");
        fs::write(&binary_path, MOCK_ORCA_SCRIPT).expect("mock script should be writable");
        let mut permissions = fs::metadata(&binary_path)
            .expect("mock script metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary_path, permissions).expect("mock script should be executable");
        let inherited_path = env::var("PATH").unwrap_or_default();
        format!("{}:{inherited_path}", bin_directory.display())
    }

    fn run_orca_script(
        arguments: &[&str],
        working_directory: &Path,
        environment_overrides: &[(&str, &str)],
    ) -> Output {
        let mut command = Command::new(runner_script_path());
        command.args(arguments).current_dir(working_directory);
        command.env_remove("ORCA_CLI_COMMAND");
        for &(variable_name, variable_value) in environment_overrides {
            command.env(variable_name, variable_value);
        }
        command
            .output()
            .expect("rig/orca/run.sh must be executable")
    }

    fn assert_orca_call_sequence(calls_text: &str) {
        let call_lines: Vec<&str> = calls_text.lines().collect();
        assert_eq!(
            call_lines.len(),
            3,
            "expected create, wait, close: {calls_text}"
        );
        let expected_worktree = format!("path:{}", repository_root().display());
        let create_fields: Vec<&str> = call_lines[0].split('\t').collect();
        assert!(!create_fields.contains(&"--focus"));
        assert!(create_fields.contains(&"--json"));
        assert!(create_fields.contains(&expected_worktree.as_str()));
        assert!(create_fields.contains(&"probe:capability_probe"));

        let wait_fields: Vec<&str> = call_lines[1].split('\t').collect();
        assert_eq!(
            wait_fields,
            vec![
                "CALL",
                "terminal",
                "wait",
                "--terminal",
                "term_mock_123",
                "--for",
                "exit",
                "--timeout-ms",
                "20000",
                "--json"
            ]
        );
        let close_fields: Vec<&str> = call_lines[2].split('\t').collect();
        assert_eq!(
            close_fields,
            vec![
                "CALL",
                "terminal",
                "close",
                "--terminal",
                "term_mock_123",
                "--tab",
                "--json"
            ]
        );
    }

    fn assert_jsonl_stdout_records(stdout_bytes: &[u8]) {
        let stdout_text = String::from_utf8(stdout_bytes.to_vec()).expect("stdout must be UTF-8");
        let stdout_lines: Vec<&str> = stdout_text.lines().collect();
        assert!(
            !stdout_lines.is_empty(),
            "stdout must contain JSONL records"
        );
        for line in &stdout_lines {
            let is_expected_record = line.starts_with("{\"kind\":\"section\"")
                || line.starts_with("{\"kind\":\"summary\"");
            assert!(is_expected_record, "unexpected stdout line: {line}");
            assert!(line.ends_with('}'), "truncated JSONL line: {line}");
        }
        assert!(
            stdout_lines
                .iter()
                .any(|line| line.contains("\"section\":\"identity\""))
        );
        assert!(
            stdout_lines
                .iter()
                .any(|line| line.starts_with("{\"kind\":\"summary\""))
        );
    }

    #[test]
    fn help_flags_print_usage_to_stdout_without_side_effects_outside_repo() {
        let outside_dir = TempTestDir::create("help");
        for help_flag in ["--help", "-h"] {
            let output = run_orca_script(
                &[help_flag],
                &outside_dir.path,
                &[("PATH", "/usr/bin:/bin"), ("HOME", "/nonexistent")],
            );
            assert!(output.status.success(), "{help_flag} must exit 0");
            assert!(
                output.stderr.is_empty(),
                "{help_flag} must keep stderr empty"
            );
            let stdout_text = String::from_utf8(output.stdout).expect("help must be UTF-8");
            assert!(stdout_text.contains("Usage:"));
            assert!(stdout_text.contains("<program>"));
            assert!(stdout_text.contains("--batch"));
            assert!(stdout_text.contains("--interactive"));
            assert!(stdout_text.contains("capability_probe"));
            assert!(!stdout_text.contains("\x1b["));
        }
        let created_entries = fs::read_dir(&outside_dir.path)
            .expect("outside dir should remain readable")
            .count();
        assert_eq!(created_entries, 0);
    }

    #[test]
    fn invalid_invocations_fail_with_recovery_hint_on_stderr_and_empty_stdout() {
        let temp_dir = TempTestDir::create("invalid");
        let invalid_cases: &[&[&str]] = &[
            &[],
            &["--unknown-flag"],
            &["bad/name"],
            &["capability_probe", "--log", "/tmp/manual.jsonl"],
            &["capability_probe", "--timeout-ms", "not-a-number"],
            &["capability_probe", "--batch", "--interactive"],
        ];
        for case_arguments in invalid_cases {
            let output = run_orca_script(
                case_arguments,
                &temp_dir.path,
                &[("NO_COLOR", "1"), ("TERM", "dumb")],
            );
            assert_eq!(output.status.code(), Some(2), "args: {case_arguments:?}");
            assert!(output.stdout.is_empty(), "args: {case_arguments:?}");
            let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
            assert!(stderr_text.contains("❌"), "stderr: {stderr_text}");
            assert!(stderr_text.contains("\n  ⚠️ "), "stderr: {stderr_text}");
            assert!(!stderr_text.contains("\x1b["), "stderr: {stderr_text}");
        }
    }

    #[test]
    fn missing_orca_binary_fails_cleanly_with_recovery_hint() {
        let temp_dir = TempTestDir::create("no-orca");
        let empty_home = temp_dir.path.join("empty-home");
        fs::create_dir_all(&empty_home).expect("empty home should be creatable");
        let output = run_orca_script(
            &["capability_probe"],
            &temp_dir.path,
            &[
                ("PATH", "/usr/bin:/bin"),
                ("HOME", empty_home.to_str().expect("UTF-8 path")),
                ("NO_COLOR", "1"),
            ],
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
        assert!(stderr_text.contains("❌"));
        assert!(stderr_text.contains("\n  ⚠️ "));
    }

    #[test]
    fn mock_orca_runs_default_batch_probe_and_streams_jsonl_sections() {
        let temp_dir = TempTestDir::create("mock-default");
        let combined_path = create_mock_orca_binary(&temp_dir.path.join("bin"));
        let calls_file = temp_dir.path.join("orca_calls.tsv");

        let output = run_orca_script(
            &["capability_probe"],
            &temp_dir.path,
            &[
                ("PATH", &combined_path),
                ("TMPDIR", temp_dir.path.to_str().expect("UTF-8")),
                ("MOCK_ORCA_CALLS_FILE", calls_file.to_str().expect("UTF-8")),
            ],
        );
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let calls_text = fs::read_to_string(&calls_file).expect("orca calls log must exist");
        assert_orca_call_sequence(&calls_text);
        assert_jsonl_stdout_records(&output.stdout);
        let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
        assert!(stderr_text.contains("✅"), "stderr: {stderr_text}");
    }

    #[test]
    fn mock_orca_quotes_special_osc52_tokens_without_duplicating_batch() {
        let temp_dir = TempTestDir::create("mock-osc52");
        let combined_path = create_mock_orca_binary(&temp_dir.path.join("bin"));
        let calls_file = temp_dir.path.join("orca_calls.tsv");

        let output = run_orca_script(
            &[
                "capability_probe",
                "--batch",
                "--osc52",
                "tok 'single' $UNEXPANDED & more",
            ],
            &temp_dir.path,
            &[
                ("PATH", &combined_path),
                ("TMPDIR", temp_dir.path.to_str().expect("UTF-8")),
                ("MOCK_ORCA_CALLS_FILE", calls_file.to_str().expect("UTF-8")),
                ("UNEXPANDED", "WRONG_IF_EXPANDED"),
            ],
        );
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout_text = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
        assert!(
            stdout_text.contains("{\"kind\":\"section\",\"section\":\"osc52\",\"emitted\":true")
        );
        let calls_text = fs::read_to_string(&calls_file).expect("calls file should exist");
        let batch_count = calls_text
            .lines()
            .next()
            .unwrap_or_default()
            .matches("--batch")
            .count();
        assert_eq!(batch_count, 1);
        assert!(!calls_text.contains("WRONG_IF_EXPANDED"));
    }

    #[test]
    fn failed_probe_preserves_log_directory_and_closes_terminal() {
        let temp_dir = TempTestDir::create("failed-probe");
        let combined_path = create_mock_orca_binary(&temp_dir.path.join("bin"));
        let calls_file = temp_dir.path.join("orca_calls_fail.tsv");
        let scratch_root = temp_dir.path.join("scratch");
        fs::create_dir_all(&scratch_root).expect("scratch dir must be creatable");

        let output = run_orca_script(
            &["capability_probe"],
            &temp_dir.path,
            &[
                ("PATH", &combined_path),
                ("TMPDIR", scratch_root.to_str().expect("UTF-8")),
                ("MOCK_ORCA_CALLS_FILE", calls_file.to_str().expect("UTF-8")),
                ("MOCK_ORCA_SKIP_EXEC", "1"),
            ],
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
        assert!(stderr_text.contains("❌"));
        assert!(stderr_text.contains("\n  ⚠️ "));

        let preserved_count = fs::read_dir(&scratch_root)
            .expect("scratch dir should be readable")
            .count();
        assert_eq!(preserved_count, 1);

        let calls_text = fs::read_to_string(&calls_file).expect("calls file must exist");
        assert!(
            calls_text.contains("CALL\tterminal\tclose\t--terminal\tterm_mock_123\t--tab\t--json")
        );
    }
}
