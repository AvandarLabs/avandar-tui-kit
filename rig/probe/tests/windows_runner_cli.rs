//! Integration tests for `rig/windows/run.sh`, `scripts/ci/check-features.sh`, and `.github/workflows/ci.yml`.

use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("rig/probe must sit two levels below workspace root")
        .to_path_buf()
}

#[test]
fn ci_and_windows_files_exist_without_sleep_or_em_dash() {
    let root = repository_root();
    for (relative_path, is_shell_script) in [
        ("rig/windows/run.sh", true),
        ("scripts/ci/check-features.sh", true),
        (".github/workflows/ci.yml", false),
    ] {
        let full_path = root.join(relative_path);
        let contents = fs::read_to_string(&full_path)
            .unwrap_or_else(|err| panic!("{} must be readable: {err}", full_path.display()));
        if is_shell_script {
            assert!(
                contents.starts_with("#!/usr/bin/env bash"),
                "{relative_path}"
            );
        }
        assert!(
            !contents.contains("sleep"),
            "{relative_path} contains sleep"
        );
        assert!(
            !contents.contains('\u{2014}'),
            "{relative_path} contains em dash"
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

    use super::repository_root;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn windows_script() -> PathBuf {
        repository_root().join("rig/windows/run.sh")
    }

    fn check_features_script() -> PathBuf {
        repository_root().join("scripts/ci/check-features.sh")
    }

    const MOCK_CARGO_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
[ "${1:-}" = "metadata" ] && { cat "$MOCK_CARGO_METADATA_FILE"; exit 0; }
{ printf 'CHECK'; for arg in "$@"; do printf '\t%s' "$arg"; done; printf '\n'; } >> "$MOCK_CALLS_FILE"
"#;

    const MOCK_GH_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
{ printf 'GH'; for arg in "$@"; do printf '\t%s' "$arg"; done; printf '\n'; } >> "$MOCK_CALLS_FILE"
case "${1:-}:${2:-}" in
  auth:status) [ "${MOCK_GH_UNAUTH:-0}" = "1" ] && exit 1; exit 0 ;;
  run:list) cat "$MOCK_GH_LIST_FILE" ;;
  run:watch) cp "$MOCK_GH_AFTER_WATCH_FILE" "$MOCK_GH_VIEW_FILE"; exit 0 ;;
  run:view)
    for arg in "$@"; do [ "$arg" = "--log" ] && { printf 'ConPTY log ok\n'; exit 0; }; done
    cat "$MOCK_GH_VIEW_FILE" ;;
  run:download)
    dir=""; prev=""
    for arg in "$@"; do [ "$prev" = "--dir" ] && dir="$arg"; prev="$arg"; done
    [ ! -e "$dir/windows-event-logs/xterm-01-probe.jsonl" ] || { printf 'file exists\n' >&2; exit 1; }
    mkdir -p "$dir/windows-event-logs"
    printf '{"kind":"summary"}\n' > "$dir/windows-event-logs/xterm-01-probe.jsonl" ;;
  *) exit 1 ;;
esac
"#;

    struct TempTestDir(PathBuf);

    impl TempTestDir {
        fn create(label: &str) -> Self {
            let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("tuikit-win-{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("temp dir creatable");
            Self(path)
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct MockCliFixture {
        temp_dir: TempTestDir,
        combined_path: String,
        metadata_file: PathBuf,
        list_file: PathBuf,
        view_file: PathBuf,
        after_watch_file: PathBuf,
        calls_file: PathBuf,
    }

    impl MockCliFixture {
        fn create(label: &str) -> Self {
            let temp_dir = TempTestDir::create(label);
            let bin_dir = temp_dir.0.join("bin");
            fs::create_dir_all(&bin_dir).expect("bin dir");
            write_executable(&bin_dir.join("gh"), MOCK_GH_SCRIPT);
            write_executable(&bin_dir.join("cargo"), MOCK_CARGO_SCRIPT);
            let host_path = env::var("PATH").unwrap_or_default();
            Self {
                metadata_file: temp_dir.0.join("metadata.json"),
                list_file: temp_dir.0.join("list.json"),
                view_file: temp_dir.0.join("view.json"),
                after_watch_file: temp_dir.0.join("after_watch.json"),
                calls_file: temp_dir.0.join("calls.tsv"),
                combined_path: format!("{}:{host_path}", bin_dir.display()),
                temp_dir,
            }
        }

        fn run_target(&self, script: &Path, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
            let mut env_pairs = vec![
                ("PATH", self.combined_path.as_str()),
                (
                    "MOCK_CARGO_METADATA_FILE",
                    self.metadata_file.to_str().expect("UTF-8"),
                ),
                ("MOCK_GH_LIST_FILE", self.list_file.to_str().expect("UTF-8")),
                ("MOCK_GH_VIEW_FILE", self.view_file.to_str().expect("UTF-8")),
                (
                    "MOCK_GH_AFTER_WATCH_FILE",
                    self.after_watch_file.to_str().expect("UTF-8"),
                ),
                ("MOCK_CALLS_FILE", self.calls_file.to_str().expect("UTF-8")),
                ("NO_COLOR", "1"),
            ];
            env_pairs.extend_from_slice(extra_env);
            run_script(script, args, &self.temp_dir.0, &env_pairs)
        }
    }

    fn write_executable(file_path: &Path, script_body: &str) {
        fs::write(file_path, script_body).expect("script writable");
        let mut permissions = fs::metadata(file_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(file_path, permissions).expect("executable");
    }

    fn create_hermetic_shell_bin(bin_dir: &Path) {
        fs::create_dir_all(bin_dir).expect("hermetic bin dir");
        let mut dirs: Vec<PathBuf> =
            env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
        dirs.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        for name in ["env", "bash"] {
            let src = dirs
                .iter()
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
                .unwrap_or_else(|| panic!("missing system tool {name}"));
            symlink(src, bin_dir.join(name)).expect("symlink");
        }
    }

    fn run_script(script: &Path, args: &[&str], cwd: &Path, envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new(script);
        command.args(args).current_dir(cwd).env_remove("NO_COLOR");
        for &(name, value) in envs {
            command.env(name, value);
        }
        command.output().expect("script must execute")
    }

    #[test]
    fn help_flags_work_outside_repo_without_external_tools() {
        let temp_dir = TempTestDir::create("help");
        let hermetic_bin = temp_dir.0.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let empty_cwd = temp_dir.0.join("empty-cwd");
        fs::create_dir_all(&empty_cwd).expect("empty cwd");
        let path_val = hermetic_bin.to_str().expect("UTF-8");

        for script in [windows_script(), check_features_script()] {
            for flag in ["--help", "-h"] {
                let out = run_script(
                    &script,
                    &[flag],
                    &empty_cwd,
                    &[("PATH", path_val), ("NO_COLOR", "1")],
                );
                assert!(out.status.success() && out.stderr.is_empty());
                let text = String::from_utf8(out.stdout).expect("UTF-8");
                for heading in ["Usage:", "Purpose:", "Options:", "Streams:", "Examples:"] {
                    assert!(text.contains(heading), "missing {heading}");
                }
                assert!(!text.contains("\x1b["));
            }
        }
    }

    #[test]
    fn invalid_cli_flags_exit_two_with_usage_and_recovery_hint() {
        let temp_dir = TempTestDir::create("invalid");
        let win_cases: &[&[&str]] = &[
            &["--unknown-flag"],
            &["--branch"],
            &["--run-id", "not-a-number"],
            &["--output-dir"],
            &["unexpected-positional"],
        ];
        for args in win_cases {
            let out = run_script(&windows_script(), args, &temp_dir.0, &[("NO_COLOR", "1")]);
            assert_eq!(out.status.code(), Some(2), "args: {args:?}");
            assert!(out.stdout.is_empty());
            let err = String::from_utf8(out.stderr).expect("UTF-8");
            assert!(err.contains("Usage:") && err.contains("\n  ⚠️ "));
        }
        let cf = run_script(
            &check_features_script(),
            &["--bad"],
            &temp_dir.0,
            &[("NO_COLOR", "1")],
        );
        assert_eq!(cf.status.code(), Some(2));
        assert!(cf.stdout.is_empty());
    }

    #[test]
    fn check_features_runs_no_default_individual_and_all_features_for_published_crates() {
        let fixture = MockCliFixture::create("check-features");
        let root_str = repository_root()
            .to_str()
            .expect("UTF-8")
            .replace('\\', "/");
        let metadata_json = format!(
            r#"{{"packages":[
              {{"name":"tui-alpha","manifest_path":"{root_str}/crates/tui-alpha/Cargo.toml","publish":null,"features":{{"default":[],"mouse":[],"paste":[]}}}},
              {{"name":"rig-probe","manifest_path":"{root_str}/rig/probe/Cargo.toml","publish":[],"features":{{"default":[],"internal":[]}}}}
            ]}}"#
        );
        fs::write(&fixture.metadata_file, metadata_json).expect("metadata");
        let out = fixture.run_target(&check_features_script(), &[], &[]);
        assert!(out.status.success());
        let calls: Vec<String> = fs::read_to_string(&fixture.calls_file)
            .expect("calls")
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(
            calls,
            vec![
                "CHECK\tcheck\t-p\ttui-alpha\t--no-default-features\t--all-targets",
                "CHECK\tcheck\t-p\ttui-alpha\t--no-default-features\t--features\tmouse\t--all-targets",
                "CHECK\tcheck\t-p\ttui-alpha\t--no-default-features\t--features\tpaste\t--all-targets",
                "CHECK\tcheck\t-p\ttui-alpha\t--all-features\t--all-targets",
            ]
        );
    }

    #[test]
    fn windows_runner_fails_cleanly_when_gh_missing_or_unauthenticated_or_no_runs() {
        let temp_dir = TempTestDir::create("missing-gh");
        let hermetic_bin = temp_dir.0.join("hermetic-bin");
        create_hermetic_shell_bin(&hermetic_bin);
        let path_val = hermetic_bin.to_str().expect("UTF-8");
        let missing = run_script(
            &windows_script(),
            &["--branch", "feat/m0-test-rig"],
            &temp_dir.0,
            &[("PATH", path_val), ("NO_COLOR", "1")],
        );
        assert_eq!(missing.status.code(), Some(1));
        assert!(missing.stdout.is_empty());

        let fixture = MockCliFixture::create("unauth-empty");
        let unauth = fixture.run_target(
            &windows_script(),
            &["--branch", "main"],
            &[("MOCK_GH_UNAUTH", "1")],
        );
        assert_eq!(unauth.status.code(), Some(1));
        fs::write(&fixture.list_file, "[]").expect("empty list");
        let empty = fixture.run_target(&windows_script(), &["--branch", "main"], &[]);
        assert_eq!(empty.status.code(), Some(1));
        assert!(empty.stdout.is_empty());
    }

    #[test]
    fn windows_runner_watches_in_progress_run_and_downloads_into_relative_dir_twice() {
        let fixture = MockCliFixture::create("watch-ok");
        let out_dir = fixture.temp_dir.0.join("downloaded");
        fs::write(&fixture.list_file, r#"[{"databaseId":4242,"status":"in_progress","conclusion":null,"headSha":"abc","url":"https://example.test/4242"}]"#).expect("list");
        fs::write(&fixture.view_file, r#"{"databaseId":4242,"headBranch":"feat/m0","status":"in_progress","conclusion":null,"headSha":"abc","url":"https://example.test/4242","jobs":[{"databaseId":9001,"name":"Windows","status":"in_progress","conclusion":null}]}"#).expect("view");
        fs::write(&fixture.after_watch_file, r#"{"databaseId":4242,"headBranch":"feat/m0","status":"completed","conclusion":"success","headSha":"abc","url":"https://example.test/4242","jobs":[{"databaseId":9001,"name":"Windows","status":"completed","conclusion":"success"},{"databaseId":9002,"name":"Linux","status":"completed","conclusion":"success"}]}"#).expect("after");

        let args = [
            "--branch",
            "feat/m0",
            "--wait",
            "--output-dir",
            "./downloaded",
        ];
        let first = fixture.run_target(&windows_script(), &args, &[]);
        let second = fixture.run_target(&windows_script(), &args, &[]);
        assert!(first.status.success() && second.status.success());
        let second_stderr = String::from_utf8(second.stderr).expect("UTF-8");
        assert!(!second_stderr.contains("No 'windows-event-logs*' artifacts"));
        let stdout_line = String::from_utf8(second.stdout).expect("UTF-8");
        assert_eq!(stdout_line.lines().count(), 1);
        assert!(stdout_line.contains("\"run_id\":4242"));
        assert!(out_dir.join("jobs/Windows-9001.log").is_file());
        assert!(
            out_dir
                .join("artifacts/windows-event-logs/xterm-01-probe.jsonl")
                .is_file()
        );
        assert!(!repository_root().join("downloaded").exists());
        assert!(
            fs::read_to_string(&fixture.calls_file)
                .expect("calls")
                .contains("GH\trun\twatch\t4242")
        );
    }

    #[test]
    fn windows_runner_exits_one_when_windows_job_fails_after_downloading_logs() {
        let fixture = MockCliFixture::create("win-fail");
        let out_dir = fixture.temp_dir.0.join("failed-out");
        let out_arg = out_dir.to_str().expect("UTF-8");
        fs::write(&fixture.view_file, r#"{"databaseId":777,"headBranch":"feat/m0","status":"completed","conclusion":"failure","headSha":"def","url":"https://example.test/777","jobs":[{"databaseId":8001,"name":"Windows","status":"completed","conclusion":"failure"}]}"#).expect("view");

        let failed = fixture.run_target(
            &windows_script(),
            &["--run-id", "777", "--output-dir", out_arg],
            &[],
        );
        assert_eq!(failed.status.code(), Some(1));
        assert!(out_dir.join("jobs/Windows-8001.log").is_file());
        assert!(
            String::from_utf8(failed.stdout)
                .expect("UTF-8")
                .contains("\"conclusion\":\"failure\"")
        );
        let err = String::from_utf8(failed.stderr).expect("UTF-8");
        assert!(err.contains("❌") && err.contains("\n  ⚠️ "));
    }
}
