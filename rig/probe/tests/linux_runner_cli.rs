//! Integration tests for the Linux floor container runner (`rig/linux/run.sh`).

use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rig/probe must sit two levels below workspace root")
        .to_path_buf()
}

fn linux_dir() -> PathBuf {
    repository_root().join("rig/linux")
}

#[test]
fn linux_files_exist_without_sleep_or_em_dash() {
    let base_dir = linux_dir();
    for (relative_name, is_shell_script) in [
        ("Dockerfile", false),
        ("entrypoint.sh", true),
        ("run.sh", true),
    ] {
        let full_path = base_dir.join(relative_name);
        let contents = fs::read_to_string(&full_path)
            .unwrap_or_else(|err| panic!("{} must be readable: {err}", full_path.display()));
        if is_shell_script {
            assert!(
                contents.starts_with("#!/usr/bin/env bash"),
                "{relative_name} must start with bash shebang"
            );
        }
        assert!(
            !contents.contains("sleep"),
            "{relative_name} must not contain sleep"
        );
        assert!(
            !contents.contains('\u{2014}'),
            "{relative_name} must not contain em dashes"
        );
    }
}

#[cfg(unix)]
mod unix_tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{linux_dir, repository_root};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn runner_script_path() -> PathBuf {
        linux_dir().join("run.sh")
    }

    const MOCK_DOCKER_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
if [ -n "${MOCK_DOCKER_CALLS_FILE:-}" ]; then
  { printf 'CALL'; for item in "$@"; do printf '\t%s' "$item"; done; printf '\n'; } >> "$MOCK_DOCKER_CALLS_FILE"
fi
case "${1:-}:${2:-}" in
  info:*)
    [ "${MOCK_DOCKER_INFO_FAIL:-0}" = "1" ] && exit 1
    printf 'aarch64\n' ;;
  image:inspect)
    [ "${MOCK_DOCKER_IMAGE_MISSING:-0}" = "1" ] && exit 1
    printf '[]\n' ;;
  volume:inspect)
    [ "${MOCK_DOCKER_VOLUME_MISSING:-0}" = "1" ] && exit 1
    printf '[]\n' ;;
  build:*)
    printf 'Step 1/7 : FROM rust:1.88-bookworm\n'
    [ "${MOCK_DOCKER_BUILD_FAIL:-0}" = "1" ] && exit 1
    exit 0 ;;
  run:*)
    for arg in "$@"; do
      if [ "$arg" = "--entrypoint" ]; then
        printf 'volume-init-noise\n'
        exit 0
      fi
    done
    [ -n "${MOCK_DOCKER_STDOUT:-}" ] && printf '%s\n' "$MOCK_DOCKER_STDOUT"
    exit "${MOCK_DOCKER_EXIT_CODE:-0}" ;;
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
                "tuikit-linux-test-{label}-{}-{unique_id}",
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

    struct MockDockerFixture {
        temp_dir: TempTestDir,
        combined_path: String,
        calls_file: PathBuf,
    }

    impl MockDockerFixture {
        fn create(label: &str) -> Self {
            let temp_dir = TempTestDir::create(label);
            let bin_dir = temp_dir.path.join("bin");
            fs::create_dir_all(&bin_dir).expect("mock bin dir must be creatable");
            let docker_path = bin_dir.join("docker");
            fs::write(&docker_path, MOCK_DOCKER_SCRIPT).expect("mock docker writable");
            let mut permissions = fs::metadata(&docker_path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&docker_path, permissions).expect("mock docker executable");
            let inherited_path = env::var("PATH").unwrap_or_default();
            let combined_path = format!("{}:{inherited_path}", bin_dir.display());
            let calls_file = temp_dir.path.join("docker_calls.tsv");
            Self {
                temp_dir,
                combined_path,
                calls_file,
            }
        }

        fn run(&self, arguments: &[&str], extra_env: &[(&str, &str)]) -> Output {
            let mut env_pairs = vec![
                ("PATH", self.combined_path.as_str()),
                ("TMPDIR", self.temp_dir.path.to_str().expect("UTF-8")),
                (
                    "MOCK_DOCKER_CALLS_FILE",
                    self.calls_file.to_str().expect("UTF-8"),
                ),
            ];
            env_pairs.extend_from_slice(extra_env);
            run_linux_script(arguments, &self.temp_dir.path, &env_pairs)
        }

        fn read_calls(&self) -> String {
            fs::read_to_string(&self.calls_file).unwrap_or_default()
        }

        fn has_leftover_context_dirs(&self) -> bool {
            fs::read_dir(&self.temp_dir.path)
                .expect("fixture temp dir readable")
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("tuikit-linux-ctx.")
                })
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

    fn run_linux_script(
        arguments: &[&str],
        working_directory: &Path,
        environment_overrides: &[(&str, &str)],
    ) -> Output {
        let mut command = Command::new(runner_script_path());
        command
            .args(arguments)
            .current_dir(working_directory)
            .env_remove("NO_COLOR")
            .env_remove("RIG_XTERM_LOG_DIR");
        for &(variable_name, variable_value) in environment_overrides {
            command.env(variable_name, variable_value);
        }
        command
            .output()
            .expect("rig/linux/run.sh must be executable")
    }

    #[test]
    fn help_flags_print_usage_to_stdout_without_docker_outside_repo() {
        let outside_dir = TempTestDir::create("help");
        let hermetic_bin = outside_dir.path.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let working_dir = outside_dir.path.join("empty-cwd");
        fs::create_dir_all(&working_dir).expect("empty cwd creatable");
        let hermetic_path = hermetic_bin.to_str().expect("UTF-8");
        for help_args in [&["--help"][..], &["-h"][..], &["--rebuild", "--help"][..]] {
            let output = run_linux_script(
                help_args,
                &working_dir,
                &[
                    ("PATH", hermetic_path),
                    ("HOME", "/nonexistent"),
                    ("NO_COLOR", "1"),
                ],
            );
            assert!(output.status.success() && output.stderr.is_empty());
            let stdout_text = String::from_utf8(output.stdout).expect("UTF-8");
            for required in [
                "Usage:",
                "Purpose:",
                "Arguments:",
                "Options:",
                "Streams:",
                "Examples:",
                "--rebuild",
                "<command>",
            ] {
                assert!(stdout_text.contains(required), "missing {required}");
            }
            assert!(!stdout_text.contains("\x1b["));
        }
        assert_eq!(fs::read_dir(&working_dir).expect("readable").count(), 0);
    }

    #[test]
    fn invalid_invocations_exit_two_with_recovery_hint_on_stderr() {
        let temp_dir = TempTestDir::create("invalid");
        let invalid_cases: &[&[&str]] = &[
            &[],
            &["--rebuild"],
            &["--"],
            &["--unknown-flag"],
            &["-x", "rustc"],
        ];
        for case_args in invalid_cases {
            let output = run_linux_script(
                case_args,
                &temp_dir.path,
                &[("NO_COLOR", "1"), ("TERM", "dumb")],
            );
            assert_eq!(output.status.code(), Some(2), "args: {case_args:?}");
            assert!(output.stdout.is_empty(), "args: {case_args:?}");
            let stderr_text = String::from_utf8(output.stderr).expect("UTF-8");
            assert!(stderr_text.contains("Usage:"), "args: {case_args:?}");
            assert!(stderr_text.contains("❌") && stderr_text.contains("\n  ⚠️ "));
            assert!(!stderr_text.contains("\x1b["));
        }
    }

    #[test]
    fn missing_docker_or_unreachable_daemon_exits_one_with_recovery_hint() {
        let temp_dir = TempTestDir::create("no-docker");
        let hermetic_bin = temp_dir.path.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let missing_output = run_linux_script(
            &["rustc", "--version"],
            &temp_dir.path,
            &[
                ("PATH", hermetic_bin.to_str().expect("UTF-8")),
                ("NO_COLOR", "1"),
            ],
        );
        assert_eq!(missing_output.status.code(), Some(1));
        assert_eq!(missing_output.stdout, b"");
        let missing_stderr = String::from_utf8(missing_output.stderr).expect("UTF-8");
        assert!(missing_stderr.contains("docker") && missing_stderr.contains("\n  ⚠️ "));

        let fixture = MockDockerFixture::create("daemon-down");
        let daemon_output = fixture.run(
            &["rustc", "--version"],
            &[("MOCK_DOCKER_INFO_FAIL", "1"), ("NO_COLOR", "1")],
        );
        assert_eq!(daemon_output.status.code(), Some(1));
        assert_eq!(daemon_output.stdout, b"");
        let daemon_stderr = String::from_utf8(daemon_output.stderr).expect("UTF-8");
        assert!(daemon_stderr.contains("❌") && daemon_stderr.contains("\n  ⚠️ "));
    }

    fn assert_docker_run_contract(run_line: &str) {
        let fields: Vec<&str> = run_line.split('\t').collect();
        let workspace_mount = format!("{}:/workspace", repository_root().display());
        for required_flag in [
            "--rm",
            "--init",
            "--shm-size=1g",
            workspace_mount.as_str(),
            "HOME=/tmp/tuikit-home",
            "CARGO_TARGET_DIR=/var/cache/tuikit/cargo-target",
            "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
            "NO_COLOR=1",
            "RIG_XTERM_LOG_DIR=/tmp/mock-logs",
        ] {
            assert!(fields.contains(&required_flag), "missing {required_flag}");
        }
        assert!(
            fields
                .iter()
                .any(|item| item.ends_with(":/var/cache/tuikit/cargo-target"))
        );
        assert!(
            fields
                .iter()
                .any(|item| item.ends_with(":/workspace/rig/xterm/node_modules"))
        );
        assert!(run_line.ends_with("\t./rig/xterm/run.sh\t--help"));
    }

    #[test]
    fn mock_docker_builds_on_miss_and_streams_clean_stdout() {
        let fixture = MockDockerFixture::create("build-and-run");
        let output = fixture.run(
            &["./rig/xterm/run.sh", "--help"],
            &[
                ("MOCK_DOCKER_IMAGE_MISSING", "1"),
                ("MOCK_DOCKER_VOLUME_MISSING", "1"),
                ("MOCK_DOCKER_STDOUT", "{\"probe\":\"ok\"}"),
                ("NO_COLOR", "1"),
                ("RIG_XTERM_LOG_DIR", "/tmp/mock-logs"),
            ],
        );
        assert!(output.status.success());
        assert!(!fixture.has_leftover_context_dirs());
        assert_eq!(
            String::from_utf8(output.stdout).expect("UTF-8"),
            "{\"probe\":\"ok\"}\n"
        );
        let stderr_text = String::from_utf8(output.stderr).expect("UTF-8");
        assert!(stderr_text.contains("FROM rust:1.88-bookworm"));
        let calls_text = fixture.read_calls();
        assert!(
            calls_text
                .lines()
                .any(|line| line.starts_with("CALL\tbuild\t"))
        );
        let main_run_line = calls_text
            .lines()
            .rfind(|line| line.starts_with("CALL\trun\t"))
            .expect("expected docker run invocation");
        assert_docker_run_contract(main_run_line);
    }

    #[test]
    fn mock_docker_reuses_cached_image_supports_rebuild_and_propagates_exit_code() {
        let cached = MockDockerFixture::create("cached-image");
        let cached_output = cached.run(&["cargo", "test"], &[("NO_COLOR", "1")]);
        assert!(cached_output.status.success());
        assert!(!cached.read_calls().contains("CALL\tbuild\t"));

        let rebuild = MockDockerFixture::create("rebuild-image");
        let rebuild_output = rebuild.run(&["--rebuild", "cargo", "test"], &[("NO_COLOR", "1")]);
        assert!(rebuild_output.status.success());
        assert!(rebuild.read_calls().contains("CALL\tbuild\t"));
        assert!(!rebuild.has_leftover_context_dirs());

        let build_failed = MockDockerFixture::create("failed-build");
        let build_failed_output = build_failed.run(
            &["--rebuild", "cargo", "test"],
            &[("MOCK_DOCKER_BUILD_FAIL", "1"), ("NO_COLOR", "1")],
        );
        assert_eq!(build_failed_output.status.code(), Some(1));
        assert!(!build_failed.has_leftover_context_dirs());

        let failing = MockDockerFixture::create("failing-command");
        let failing_output = failing.run(
            &["cargo", "test"],
            &[("MOCK_DOCKER_EXIT_CODE", "17"), ("NO_COLOR", "1")],
        );
        assert_eq!(failing_output.status.code(), Some(17));
        let failing_stderr = String::from_utf8(failing_output.stderr).expect("UTF-8");
        assert!(failing_stderr.contains("❌") && failing_stderr.contains("\n  ⚠️ "));
    }
}
