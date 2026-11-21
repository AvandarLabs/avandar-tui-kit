//! Integration tests for the Layer 5 unfocused Orca runner (`rig/orca/run.sh`).

use std::fs;
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

#[test]
fn runner_script_exists_and_never_requests_focus_or_sleep() {
    let script_contents = fs::read_to_string(runner_script_path())
        .expect("rig/orca/run.sh must be readable from repository root");
    assert!(script_contents.starts_with("#!/usr/bin/env bash"));
    assert!(!script_contents.contains("--focus"));
    assert!(!script_contents.contains("sleep"));
    assert!(!script_contents.contains('\u{2014}'));
}

#[cfg(unix)]
mod unix_tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
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
stub_probe() {
  local log_out=""
  while [ "$#" -gt 0 ]; do
    case "$1" in --log) log_out="$2"; shift 2 ;; --batch) return 99 ;; *) shift ;; esac
  done
  printf '{"kind":"summary","mode":"interactive"}\n{"kind":"ready"}\n' > "$log_out" && : > "${log_out}.ready"
}
case "${1:-}:${2:-}" in
  terminal:create)
    shift 2; command_text=""
    while [ "$#" -gt 0 ]; do case "$1" in --command) command_text="${2:-}"; shift 2 ;; *) shift ;; esac; done
    if [ "${MOCK_ORCA_STUB_INTERACTIVE:-0}" = "1" ] && [ -n "$command_text" ]; then
      ( eval "stub_probe ${command_text#*\'*\'}" </dev/null >/dev/null 2>&1 ) || true
    elif [ "${MOCK_ORCA_SKIP_EXEC:-0}" != "1" ] && [ -n "$command_text" ]; then
      ( /bin/sh -c "$command_text" </dev/null >/dev/null 2>&1 ) || true
    fi
    printf '{"id":"req-create","ok":true,"result":{"terminal":{"handle":"term_mock_123","surface":"tab"}}}\n' ;;
  terminal:wait)
    printf '{"id":"req-wait","ok":true,"result":{"wait":{"handle":"term_mock_123","condition":"exit","satisfied":true,"status":"exited","exitCode":0}}}\n' ;;
  terminal:close)
    printf '{"id":"req-close","ok":true,"result":{"closed":true}}\n' ;;
  *) exit 1 ;;
esac
"#;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn create(label: &str) -> Self {
            let unique_id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir_name = format!(
                "tuikit-orca-test-{label}-{}-{unique_id}",
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

    struct MockOrcaFixture {
        temp_dir: TempTestDir,
        combined_path: String,
        calls_file: PathBuf,
        scratch_root: PathBuf,
    }

    impl MockOrcaFixture {
        fn create(label: &str) -> Self {
            let temp_dir = TempTestDir::create(label);
            let bin_directory = temp_dir.path.join("bin");
            fs::create_dir_all(&bin_directory).expect("mock bin dir must be creatable");
            let binary_path = bin_directory.join("orca");
            fs::write(&binary_path, MOCK_ORCA_SCRIPT).expect("mock script should be writable");
            let mut permissions = fs::metadata(&binary_path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&binary_path, permissions).expect("mock script must be executable");
            let inherited_path = env::var("PATH").unwrap_or_default();
            let combined_path = format!("{}:{inherited_path}", bin_directory.display());
            let calls_file = temp_dir.path.join("orca_calls.tsv");
            let scratch_root = temp_dir.path.join("scratch");
            fs::create_dir_all(&scratch_root).expect("scratch dir must be creatable");
            Self {
                temp_dir,
                combined_path,
                calls_file,
                scratch_root,
            }
        }

        fn run(&self, arguments: &[&str], extra_env: &[(&str, &str)]) -> Output {
            let mut env_pairs = vec![
                ("PATH", self.combined_path.as_str()),
                ("TMPDIR", self.scratch_root.to_str().expect("UTF-8")),
                (
                    "MOCK_ORCA_CALLS_FILE",
                    self.calls_file.to_str().expect("UTF-8"),
                ),
            ];
            env_pairs.extend_from_slice(extra_env);
            run_orca_script(arguments, &self.temp_dir.path, &env_pairs)
        }

        fn read_calls(&self) -> String {
            fs::read_to_string(&self.calls_file).expect("orca calls file must exist")
        }

        fn scratch_entry_count(&self) -> usize {
            fs::read_dir(&self.scratch_root)
                .expect("scratch dir readable")
                .count()
        }
    }

    fn create_hermetic_shell_bin(bin_directory: &Path) {
        fs::create_dir_all(bin_directory).expect("hermetic bin dir must be creatable");
        let mut search_dirs: Vec<PathBuf> =
            env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
        search_dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        for tool_name in ["env", "bash", "uname"] {
            let source_path = search_dirs
                .iter()
                .map(|directory| directory.join(tool_name))
                .find(|candidate| candidate.is_file())
                .unwrap_or_else(|| panic!("required system tool {tool_name} not found"));
            symlink(source_path, bin_directory.join(tool_name)).expect("symlink must succeed");
        }
    }

    fn run_orca_script(
        arguments: &[&str],
        working_directory: &Path,
        environment_overrides: &[(&str, &str)],
    ) -> Output {
        let mut command = Command::new(runner_script_path());
        command
            .args(arguments)
            .current_dir(working_directory)
            .env_remove("ORCA_CLI_COMMAND")
            .env_remove("ORCA_DEV_REPO_ROOT")
            .env_remove("ORCA_SESSION_ID");
        for &(variable_name, variable_value) in environment_overrides {
            command.env(variable_name, variable_value);
        }
        command
            .output()
            .expect("rig/orca/run.sh must be executable")
    }

    fn assert_run_succeeded(output: &Output) {
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn assert_orca_call_sequence(calls_text: &str, expected_timeout_ms: &str) {
        let call_lines: Vec<&str> = calls_text.lines().collect();
        assert_eq!(
            call_lines.len(),
            3,
            "expected create, wait, close: {calls_text}"
        );
        let expected_worktree = format!("path:{}", repository_root().display());
        let create_fields: Vec<&str> = call_lines[0].split('\t').collect();
        assert!(!create_fields.contains(&"--focus") && create_fields.contains(&"--json"));
        assert!(create_fields.contains(&expected_worktree.as_str()));
        assert!(create_fields.contains(&"probe:capability_probe"));
        let expected_wait = format!(
            "CALL\tterminal\twait\t--terminal\tterm_mock_123\t--for\texit\t--timeout-ms\t{expected_timeout_ms}\t--json"
        );
        assert_eq!(call_lines[1], expected_wait);
        assert_eq!(
            call_lines[2],
            "CALL\tterminal\tclose\t--terminal\tterm_mock_123\t--tab\t--json"
        );
    }

    fn assert_jsonl_stdout_records(stdout_bytes: &[u8]) {
        let stdout_text = String::from_utf8(stdout_bytes.to_vec()).expect("stdout must be UTF-8");
        let stdout_lines: Vec<&str> = stdout_text.lines().collect();
        assert_ne!(stdout_lines, Vec::<&str>::new());
        for line in &stdout_lines {
            let is_valid = line.starts_with("{\"kind\":\"section\"")
                || line.starts_with("{\"kind\":\"summary\"");
            assert!(
                is_valid && line.ends_with('}'),
                "unexpected stdout line: {line}"
            );
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
        let hermetic_bin = outside_dir.path.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let working_dir = outside_dir.path.join("empty-cwd");
        fs::create_dir_all(&working_dir).expect("empty cwd should be creatable");
        let hermetic_path = hermetic_bin.to_str().expect("UTF-8");
        for help_flag in ["--help", "-h"] {
            let output = run_orca_script(
                &[help_flag],
                &working_dir,
                &[("PATH", hermetic_path), ("HOME", "/nonexistent")],
            );
            assert!(output.status.success() && output.stderr.is_empty());
            let stdout_text = String::from_utf8(output.stdout).expect("help must be UTF-8");
            for token in [
                "Usage:",
                "<program>",
                "--batch",
                "--interactive",
                "capability_probe",
            ] {
                assert!(stdout_text.contains(token), "missing {token}");
            }
            assert!(!stdout_text.contains("\x1b["));
        }
        assert_eq!(fs::read_dir(&working_dir).expect("readable").count(), 0);
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
            assert!(stderr_text.contains("❌") && stderr_text.contains("\n  ⚠️ "));
            assert!(!stderr_text.contains("\x1b["));
        }
    }

    #[test]
    fn missing_orca_binary_fails_cleanly_with_recovery_hint() {
        let temp_dir = TempTestDir::create("no-orca");
        let hermetic_bin = temp_dir.path.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let empty_home = temp_dir.path.join("empty-home");
        fs::create_dir_all(&empty_home).expect("empty home should be creatable");
        let output = run_orca_script(
            &["capability_probe"],
            &temp_dir.path,
            &[
                ("PATH", hermetic_bin.to_str().expect("UTF-8")),
                ("HOME", empty_home.to_str().expect("UTF-8")),
                ("NO_COLOR", "1"),
            ],
        );
        assert!(!output.status.success() && output.stdout.is_empty());
        let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
        assert!(stderr_text.contains("Could not find the 'orca' CLI"));
        assert!(stderr_text.contains("❌") && stderr_text.contains("\n  ⚠️ "));
    }

    #[test]
    fn default_batch_probe_emits_jsonl_stdout_with_clean_scratch_directory() {
        let fixture = MockOrcaFixture::create("mock-default");
        let output = fixture.run(&["capability_probe"], &[]);
        assert_run_succeeded(&output);
        assert_orca_call_sequence(&fixture.read_calls(), "20000");
        assert_jsonl_stdout_records(&output.stdout);
        assert!(String::from_utf8_lossy(&output.stderr).contains("✅"));
        assert_eq!(fixture.scratch_entry_count(), 0);
    }

    #[test]
    fn mock_orca_quotes_special_osc52_tokens_without_duplicating_batch() {
        let fixture = MockOrcaFixture::create("mock-osc52");
        let output = fixture.run(
            &[
                "capability_probe",
                "--batch",
                "--osc52",
                "tok 'single' $UNEXPANDED & more",
            ],
            &[("UNEXPANDED", "WRONG_IF_EXPANDED")],
        );
        assert_run_succeeded(&output);
        let stdout_text = String::from_utf8(output.stdout).expect("stdout must be UTF-8");
        assert!(
            stdout_text.contains("{\"kind\":\"section\",\"section\":\"osc52\",\"emitted\":true")
        );
        let calls_text = fixture.read_calls();
        assert_eq!(
            calls_text
                .lines()
                .next()
                .unwrap_or_default()
                .matches("--batch")
                .count(),
            1
        );
        assert!(!calls_text.contains("WRONG_IF_EXPANDED"));
    }

    #[test]
    fn mock_orca_supports_interactive_mode_custom_timeout_and_relative_target_dir() {
        let fixture = MockOrcaFixture::create("mock-interactive");
        symlink(
            repository_root().join("target"),
            fixture.temp_dir.path.join("rel-target"),
        )
        .expect("target symlink must work");
        let output = fixture.run(
            &[
                "capability_probe",
                "--interactive",
                "--timeout-ms",
                "45000",
                "--push-kitty",
            ],
            &[
                ("CARGO_TARGET_DIR", "rel-target"),
                ("MOCK_ORCA_STUB_INTERACTIVE", "1"),
            ],
        );
        assert_run_succeeded(&output);
        let calls_text = fixture.read_calls();
        assert_orca_call_sequence(&calls_text, "45000");
        let create_line = calls_text.lines().next().unwrap_or_default();
        assert!(!create_line.contains("--batch") && create_line.contains("--push-kitty"));
        assert!(!create_line.contains("'rel-target/"));
    }

    #[test]
    fn failed_probe_preserves_log_directory_and_closes_terminal() {
        let fixture = MockOrcaFixture::create("failed-probe");
        let output = fixture.run(&["capability_probe"], &[("MOCK_ORCA_SKIP_EXEC", "1")]);
        assert!(!output.status.success() && output.stdout.is_empty());
        let stderr_text = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
        assert!(stderr_text.contains("❌") && stderr_text.contains("\n  ⚠️ "));
        assert_eq!(fixture.scratch_entry_count(), 1);
        assert!(
            fixture
                .read_calls()
                .contains("CALL\tterminal\tclose\t--terminal\tterm_mock_123\t--tab\t--json")
        );
    }
}
