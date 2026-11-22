//! Integration tests for `rig/tmux/*.conf` and `rig/orca/run.sh --tmux <stock|full>`.

use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rig/probe must sit two levels below workspace root")
        .to_path_buf()
}

fn tmux_config_path(mode: &str) -> PathBuf {
    repository_root()
        .join("rig/tmux")
        .join(format!("{mode}.conf"))
}

#[test]
fn stock_tmux_config_exists_and_is_zero_bytes() {
    let stock_bytes =
        fs::read(tmux_config_path("stock")).expect("rig/tmux/stock.conf must exist in repository");
    assert!(stock_bytes.is_empty(), "stock.conf must be 0 bytes");
}

#[test]
fn full_tmux_config_sets_all_five_required_options_without_em_dashes() {
    let full_contents = fs::read_to_string(tmux_config_path("full"))
        .expect("rig/tmux/full.conf must exist in repository");
    assert!(!full_contents.contains('\u{2014}'));
    let lines: Vec<&str> = full_contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let expected = [
        "set -g mouse on",
        "set -g set-clipboard on",
        "set -g allow-passthrough on",
        "set -g extended-keys on",
        "set -g focus-events on",
    ];
    assert_eq!(lines, expected);
}

#[cfg(unix)]
mod unix_tests {
    use std::env;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{repository_root, tmux_config_path};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    // Cargo releases its lock before the runner checks and spawns the example.
    // Own that shared binary through the entire subprocess, including spawn.
    static RUNNER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const MOCK_ORCA_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
[ -n "${MOCK_ORCA_CALLS_FILE:-}" ] && { printf 'CALL'; printf '\t%s' "$@"; printf '\n'; } >> "$MOCK_ORCA_CALLS_FILE"
case "${1:-}:${2:-}" in
  terminal:create)
    shift 2; cmd=""
    while [ "$#" -gt 0 ]; do case "$1" in --command) cmd="${2:-}"; shift 2 ;; *) shift ;; esac; done
    [ "${MOCK_ORCA_SKIP_EXEC:-0}" != "1" ] && [ -n "$cmd" ] && { ( /bin/sh -c "$cmd" </dev/null >/dev/null 2>&1 ) || true; }
    printf '{"id":"req-create","ok":true,\"result\":{\"terminal\":{\"handle\":\"term_tmux_123\",\"surface\":\"tab\"}}}\n' ;;
  terminal:wait)
    printf '{"id":"req-wait","ok":true,"result":{"wait":{"handle":"term_tmux_123","condition":"exit","satisfied":true,"status":"exited","exitCode":0}}}\n' ;;
  terminal:close) printf '{"id":"req-close","ok":true,"result":{"closed":true}}\n' ;;
  *) exit 1 ;;
esac
"#;

    const MOCK_TMUX_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
[ -n "${MOCK_TMUX_CALLS_FILE:-}" ] && { printf 'CALL'; printf '\t%s' "$@"; printf '\n'; } >> "$MOCK_TMUX_CALLS_FILE"
sock=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -L|-f) [ "$1" = "-L" ] && sock="$2"; shift 2 ;;
    new-session)
      shift
      while [ "$#" -gt 0 ]; do case "$1" in -x|-y) shift 2 ;; -*) shift ;; *) break ;; esac; done
      TMUX="/tmp/mock-${sock},999,0" TERM="tmux-256color" /bin/sh -c "$1"; exit 0 ;;
    kill-server) exit 0 ;;
    *) shift ;;
  esac
done
"#;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn create(label: &str) -> Self {
            let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "tuikit-orca-tmux-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp test dir must create");
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct MockTmuxFixture {
        temp_dir: TempTestDir,
        combined_path: String,
        orca_calls: PathBuf,
        tmux_calls: PathBuf,
        scratch_root: PathBuf,
    }

    impl MockTmuxFixture {
        fn create(label: &str) -> Self {
            let temp_dir = TempTestDir::create(label);
            let bin_dir = temp_dir.path.join("bin");
            fs::create_dir_all(&bin_dir).expect("mock bin dir must create");
            write_executable(&bin_dir.join("orca"), MOCK_ORCA_SCRIPT);
            write_executable(&bin_dir.join("tmux"), MOCK_TMUX_SCRIPT);
            let combined_path = format!(
                "{}:{}",
                bin_dir.display(),
                env::var("PATH").unwrap_or_default()
            );
            let orca_calls = temp_dir.path.join("orca_calls.tsv");
            let tmux_calls = temp_dir.path.join("tmux_calls.tsv");
            let scratch_root = temp_dir.path.join("scratch");
            fs::create_dir_all(&scratch_root).expect("scratch dir must create");
            Self {
                temp_dir,
                combined_path,
                orca_calls,
                tmux_calls,
                scratch_root,
            }
        }

        fn run(&self, arguments: &[&str], extra_env: &[(&str, &str)]) -> Output {
            let mut env_pairs = vec![
                ("PATH", self.combined_path.as_str()),
                ("TMPDIR", self.scratch_root.to_str().expect("UTF-8")),
                (
                    "MOCK_ORCA_CALLS_FILE",
                    self.orca_calls.to_str().expect("UTF-8"),
                ),
                (
                    "MOCK_TMUX_CALLS_FILE",
                    self.tmux_calls.to_str().expect("UTF-8"),
                ),
            ];
            env_pairs.extend_from_slice(extra_env);
            run_orca_script(arguments, &self.temp_dir.path, &env_pairs)
        }
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).expect("executable script must write");
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("chmod +x must succeed");
    }

    fn run_orca_script(args: &[&str], cwd: &Path, env_overrides: &[(&str, &str)]) -> Output {
        let _runner_guard = RUNNER_LOCK
            .lock()
            .expect("runner lock must not be poisoned");
        let mut command = Command::new(repository_root().join("rig/orca/run.sh"));
        command
            .args(args)
            .current_dir(cwd)
            .env_remove("ORCA_CLI_COMMAND")
            .env_remove("ORCA_DEV_REPO_ROOT")
            .env_remove("ORCA_SESSION_ID");
        for &(key, value) in env_overrides {
            command.env(key, value);
        }
        command.output().expect("rig/orca/run.sh must execute")
    }

    fn create_hermetic_bin_without_tmux(bin_dir: &Path) {
        fs::create_dir_all(bin_dir).expect("hermetic bin dir must create");
        write_executable(&bin_dir.join("orca"), MOCK_ORCA_SCRIPT);
        let mut search_dirs: Vec<PathBuf> =
            env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
        search_dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        for tool_name in ["env", "bash", "uname"] {
            let source = search_dirs
                .iter()
                .map(|dir| dir.join(tool_name))
                .find(|candidate| candidate.is_file())
                .unwrap_or_else(|| panic!("required system tool {tool_name} not found"));
            symlink(source, bin_dir.join(tool_name)).expect("symlink must succeed");
        }
    }

    fn extract_tmux_socket_and_conf(call_line: &str) -> (&str, Option<&str>) {
        let fields: Vec<&str> = call_line.split('\t').collect();
        let (mut socket, mut config, mut index) = ("", None, 0);
        while index + 1 < fields.len() {
            match fields[index] {
                "-L" => socket = fields[index + 1],
                "-f" => config = Some(fields[index + 1]),
                _ => {}
            }
            index += 1;
        }
        (socket, config)
    }

    fn assert_real_tmux_parses_config(mode: &str) {
        let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let sock = format!("tuikit-cfg-{}-{id}", std::process::id());
        let conf = tmux_config_path(mode);
        let tmux_cmds = "start-server ; display-message -p #{socket_path} ; show-options -s ; show-options -g ; show-options -gw ; kill-server".split_whitespace();
        let output = Command::new("tmux")
            .env_remove("TMUX")
            .args(["-L", &sock, "-f"])
            .arg(&conf)
            .args(tmux_cmds)
            .output()
            .expect("tmux config check must execute");
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr_text.is_empty(),
            "{stderr_text}"
        );
        let stdout_text = String::from_utf8_lossy(&output.stdout);
        if let Some(socket_file) = stdout_text.lines().next() {
            let _ = fs::remove_file(socket_file);
        }
        if mode == "full" {
            for opt in [
                "mouse on",
                "set-clipboard on",
                "allow-passthrough on",
                "focus-events on",
                "extended-keys on",
            ] {
                assert!(
                    stdout_text.contains(opt),
                    "missing {opt} in:\n{stdout_text}"
                );
            }
        }
        let leftover = Command::new("tmux")
            .args(["-L", &sock, "list-sessions"])
            .output()
            .expect("exec");
        assert!(!leftover.status.success());
    }

    #[test]
    fn tmux_binary_parses_stock_and_full_configs_cleanly_when_installed() {
        let Ok(version_out) = Command::new("tmux").arg("-V").output() else {
            return;
        };
        if version_out.status.success() {
            assert_real_tmux_parses_config("stock");
            assert_real_tmux_parses_config("full");
        }
    }

    #[test]
    fn help_documents_tmux_option_and_example() {
        let temp_dir = TempTestDir::create("help-tmux");
        let out = run_orca_script(
            &["--help"],
            &temp_dir.path,
            &[("NO_COLOR", "1"), ("TERM", "dumb")],
        );
        assert!(out.status.success() && out.stderr.is_empty());
        let stdout_text = String::from_utf8(out.stdout).expect("UTF-8");
        assert!(stdout_text.contains("--tmux <stock|full>"));
        assert!(stdout_text.contains("just orca --tmux full capability_probe"));
        assert!(!stdout_text.contains('\u{2014}'));
    }

    #[test]
    fn invalid_tmux_arguments_and_missing_tmux_binary_fail_with_recovery_hints() {
        let temp_dir = TempTestDir::create("invalid-tmux");
        let invalid_cases: &[&[&str]] = &[
            &["--tmux"],
            &["--tmux", "stock"],
            &["--tmux", "invalid", "capability_probe"],
            &["capability_probe", "--tmux"],
            &["capability_probe", "--tmux", "invalid"],
            &["capability_probe", "--tmux=invalid"],
            &["--tmux", "stock", "--tmux", "full", "capability_probe"],
        ];
        for case_args in invalid_cases {
            let out = run_orca_script(case_args, &temp_dir.path, &[("NO_COLOR", "1")]);
            assert_eq!(out.status.code(), Some(2), "args: {case_args:?}");
            assert!(out.stdout.is_empty(), "args: {case_args:?}");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(stderr.contains("\u{274c}") && stderr.contains("\n  \u{26a0}\u{fe0f} "));
        }
        let no_tmux_bin = temp_dir.path.join("no-tmux-bin");
        create_hermetic_bin_without_tmux(&no_tmux_bin);
        let out = run_orca_script(
            &["--tmux", "full", "capability_probe"],
            &temp_dir.path,
            &[
                ("PATH", no_tmux_bin.to_str().expect("UTF-8")),
                ("NO_COLOR", "1"),
            ],
        );
        assert!(!out.status.success() && out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("tmux")
                && stderr.contains("\u{274c}")
                && stderr.contains("\n  \u{26a0}\u{fe0f} ")
        );
    }

    fn assert_mock_tmux_mode_run(mode: &str, args: &[&str]) -> String {
        let fixture = MockTmuxFixture::create(mode);
        let output = fixture.run(args, &[]);
        assert!(
            output.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout_text = String::from_utf8(output.stdout).expect("UTF-8");
        assert!(
            stdout_text.contains(r#""is_tmux":true"#)
                && stdout_text.contains(r#""term":"tmux-256color""#)
        );
        let tmux_calls = fs::read_to_string(&fixture.tmux_calls).expect("tmux calls recorded");
        let lines: Vec<&str> = tmux_calls.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "expected new-session and kill-server: {tmux_calls}"
        );
        let (new_sock, new_conf) = extract_tmux_socket_and_conf(lines[0]);
        let (kill_sock, _) = extract_tmux_socket_and_conf(lines[1]);
        assert!(new_sock.starts_with("tuikit-orca-") && new_sock == kill_sock);
        assert_eq!(
            new_conf,
            Some(tmux_config_path(mode).to_str().expect("UTF-8"))
        );
        assert!(lines[1].ends_with("\tkill-server"));
        stdout_text
    }

    #[test]
    fn mock_orca_and_tmux_run_stock_and_full_modes_and_kill_private_socket() {
        let _ = assert_mock_tmux_mode_run("stock", &["--tmux", "stock", "capability_probe"]);
        let full_stdout = assert_mock_tmux_mode_run(
            "full",
            &[
                "capability_probe",
                "--tmux",
                "full",
                "--osc52",
                "tok-tmux",
                "--osc52-form",
                "tmux",
            ],
        );
        assert!(full_stdout.contains(
            r#"{"kind":"section","section":"osc52","emitted":true,"plain_sent":false,"tmux_sent":true}"#
        ));
    }

    #[test]
    fn failed_tmux_probe_still_kills_private_socket_and_closes_orca_terminal() {
        let fixture = MockTmuxFixture::create("failed-tmux");
        let output = fixture.run(
            &["--tmux", "full", "capability_probe"],
            &[("MOCK_ORCA_SKIP_EXEC", "1")],
        );
        assert!(!output.status.success() && output.stdout.is_empty());
        let tmux_calls = fs::read_to_string(&fixture.tmux_calls).expect("tmux calls recorded");
        assert!(
            tmux_calls.contains("\tkill-server"),
            "tmux_calls: {tmux_calls}"
        );
        let orca_calls = fs::read_to_string(&fixture.orca_calls).expect("orca calls recorded");
        assert!(
            orca_calls.contains("CALL\tterminal\tclose\t--terminal\tterm_tmux_123\t--tab\t--json")
        );
    }
}
