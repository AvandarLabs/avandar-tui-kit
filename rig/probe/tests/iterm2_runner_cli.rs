//! Integration tests for the Layer 6 Tart + VNC iTerm2 runner (`rig/iterm2/run.sh`).

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
    repository_root().join("rig/iterm2/run.sh")
}

fn collect_files_recursively(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory).expect("directory must be readable");
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.file_name().and_then(|n| n.to_str()) == Some("__pycache__") {
            continue;
        }
        if path.is_dir() {
            collect_files_recursively(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn runner_files_and_justfile_exist_with_zero_forbidden_tokens() {
    let script_contents = fs::read_to_string(runner_script_path())
        .expect("rig/iterm2/run.sh must be readable from repository root");
    assert!(script_contents.starts_with("#!/usr/bin/env bash"));

    let justfile_text =
        fs::read_to_string(repository_root().join("justfile")).expect("justfile must exist");
    assert!(justfile_text.contains("iterm2 *args:"));
    assert!(justfile_text.contains("./rig/iterm2/run.sh \"$@\""));

    let forbidden_delay = concat!("sl", "eep");
    let mut files = Vec::new();
    collect_files_recursively(&repository_root().join("rig/iterm2"), &mut files);
    assert!(!files.is_empty());
    for file_path in files {
        let text = fs::read_to_string(&file_path)
            .unwrap_or_else(|_| panic!("expected UTF-8 text file at {}", file_path.display()));
        assert!(
            !text.contains(forbidden_delay),
            "forbidden fixed delay word in {}",
            file_path.display()
        );
        assert!(
            !text.contains('\u{2014}'),
            "em dash found in {}",
            file_path.display()
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

    use super::runner_script_path;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    const MOCK_TART_PY: &str = r#"#!/usr/bin/env python3
import os, pathlib, re, socket, subprocess, sys, threading

calls_path = os.environ.get("MOCK_TART_CALLS_FILE", "")
if calls_path:
    with open(calls_path, "a", encoding="utf-8") as out:
        out.write("TART\t" + "\t".join(sys.argv[1:]) + "\n")

sub = sys.argv[1] if len(sys.argv) > 1 else ""
if sub == "list":
    print('[{"Source":"local","Name":"tuikit-macos","State":"stopped"}]')
    sys.exit(0)
if sub == "stop":
    state_file = os.environ.get("MOCK_TART_STOP_FLAG", "")
    if state_file:
        pathlib.Path(state_file).write_text("stopped", encoding="utf-8")
    sys.exit(0)
if sub == "run":
    shared = ""
    for arg in sys.argv[2:]:
        if arg.startswith("--dir=tuikit:"):
            shared = arg.split(":", 1)[1]
    pathlib.Path(os.environ["MOCK_TART_SHARED_PTR"]).write_text(shared, encoding="utf-8")
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.bind(("127.0.0.1", 0))
    srv.listen(8)
    port = srv.getsockname()[1]
    stop_flag = os.environ["MOCK_TART_STOP_FLAG"]
    def accept_loop():
        srv.settimeout(0.1)
        while not os.path.exists(stop_flag):
            try:
                conn, _ = srv.accept()
                conn.sendall(b"RFB 003.008\n")
                conn.close()
            except OSError:
                pass
        srv.close()
    print(f"VNC server is running at vnc://:mockpw@127.0.0.1:{port}", flush=True)
    accept_loop()
    sys.exit(0)
if sub == "exec":
    shared = pathlib.Path(os.environ["MOCK_TART_SHARED_PTR"]).read_text(encoding="utf-8")
    cmd = sys.argv[-1]
    if "MOCK_ITERM2_EXEC_HOOK" in cmd or "open -a" in cmd:
        if os.environ.get("MOCK_TART_SKIP_PROBE") == "1":
            sys.exit(0)
        m = re.search(r"(/[^\s'\"]+\.jsonl)", cmd)
        if m:
            rel = pathlib.Path(m.group(1)).name
            log_file = pathlib.Path(shared) / rel
            log_file.write_text('{"kind":"section","section":"identity","xtversion":"iTerm2 3.7.7"}\n{"kind":"ready"}\n{"kind":"summary","mode":"batch"}\n', encoding="utf-8")
            pathlib.Path(str(log_file) + ".ready").write_text("", encoding="utf-8")
            stem = rel[:-6]
            (pathlib.Path(shared) / f"{stem}.exit.json").write_text('{"exitCode":0,"signal":0,"decrqm":"\\u001b[?1000;2$y\\u001b[?1002;2$y\\u001b[?1003;2$y\\u001b[?1006;2$y\\u001b[?2004;2$y\\u001b[?1004;2$y"}', encoding="utf-8")
    elif "CGWindowListCopyWindowInfo" in cmd:
        print('[{"kCGWindowOwnerName":"iTerm2","kCGWindowLayer":0,"kCGWindowBounds":{"X":93,"Y":168,"Width":725,"Height":543}}]')
    elif "CGEventSourceFlagsState" in cmd:
        print("0x80120")
    sys.exit(0)
sys.exit(0)
"#;

    const MOCK_VNCDO_SH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
if [ -n "${MOCK_TART_CALLS_FILE:-}" ]; then
  { printf 'VNCDO'; for a in "$@"; do printf '\t%s' "$a"; done; printf '\n'; } >> "$MOCK_TART_CALLS_FILE"
fi
prev=""
for arg in "$@"; do
  if [ "$prev" = "capture" ]; then
    printf '\x89PNG\r\n\x1a\n' > "$arg"
  fi
  prev="$arg"
done
"#;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn create(label: &str) -> Self {
            let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "tuikit-iterm2-test-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp dir must be creatable");
            Self { path }
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).expect("script should be writable");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod +x must succeed");
    }

    fn create_hermetic_bin(bin_dir: &Path, tools: &[&str]) {
        fs::create_dir_all(bin_dir).expect("hermetic bin dir");
        let mut search: Vec<PathBuf> =
            env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
        search.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        for tool in tools {
            let found = search
                .iter()
                .map(|dir| dir.join(tool))
                .find(|candidate| candidate.is_file())
                .unwrap_or_else(|| panic!("missing required host tool {tool}"));
            symlink(found, bin_dir.join(tool)).expect("symlink tool");
        }
    }

    fn run_iterm2_script(args: &[&str], cwd: &Path, env_overrides: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(runner_script_path());
        cmd.args(args)
            .current_dir(cwd)
            .env_remove("RIG_ITERM2_TART_BIN")
            .env_remove("RIG_ITERM2_VNCDO_BIN")
            .env_remove("RIG_ITERM2_PROGRAM_BIN")
            .env_remove("RIG_ITERM2_LOG_DIR");
        for &(key, value) in env_overrides {
            cmd.env(key, value);
        }
        cmd.output().expect("rig/iterm2/run.sh must execute")
    }

    #[test]
    fn help_flags_print_usage_to_stdout_without_side_effects_outside_repo() {
        let temp = TempTestDir::create("help");
        let hermetic = temp.path.join("bin");
        create_hermetic_bin(&hermetic, &["env", "bash", "uname"]);
        let empty_cwd = temp.path.join("cwd");
        fs::create_dir_all(&empty_cwd).expect("cwd");
        let path_str = hermetic.to_str().expect("utf8");
        for flag in ["--help", "-h"] {
            let out = run_iterm2_script(
                &[flag],
                &empty_cwd,
                &[("PATH", path_str), ("HOME", "/nonexistent")],
            );
            assert!(out.status.success() && out.stderr.is_empty());
            let stdout_text = String::from_utf8(out.stdout).expect("utf8");
            for required in [
                "Usage:",
                "<program>",
                "Scenarios:",
                "matrix",
                "capability_probe",
            ] {
                assert!(stdout_text.contains(required), "missing {required}");
            }
            assert!(!stdout_text.contains("\x1b["));
        }
        assert!(fs::read_dir(&empty_cwd).expect("read cwd").next().is_none());
    }

    #[test]
    fn invalid_invocations_exit_two_with_recovery_hint_on_stderr() {
        let temp = TempTestDir::create("invalid");
        let cases: &[&[&str]] = &[
            &[],
            &["--unknown-flag"],
            &["bad/name"],
            &["capability_probe", "unknown_scenario"],
            &["capability_probe", "--tmux", "invalid_mode"],
            &["capability_probe", "--log", "/tmp/manual.jsonl"],
        ];
        for args in cases {
            let out = run_iterm2_script(args, &temp.path, &[("NO_COLOR", "1"), ("TERM", "dumb")]);
            assert_eq!(out.status.code(), Some(2), "args: {args:?}");
            assert!(out.stdout.is_empty(), "args: {args:?}");
            let stderr_text = String::from_utf8(out.stderr).expect("utf8");
            assert!(
                stderr_text.contains("\u{274c}") && stderr_text.contains("\n  \u{26a0}\u{fe0f} ")
            );
            assert!(!stderr_text.contains("\x1b["));
        }
    }

    #[test]
    fn missing_tart_or_vncdo_fails_with_exit_one_and_recovery_hint() {
        let temp = TempTestDir::create("missing-tools");
        let bin_no_tart = temp.path.join("bin-no-tart");
        create_hermetic_bin(&bin_no_tart, &["env", "bash", "uname", "python3"]);
        let out_no_tart = run_iterm2_script(
            &["capability_probe", "batch"],
            &temp.path,
            &[
                ("PATH", bin_no_tart.to_str().expect("utf8")),
                ("HOME", "/nonexistent"),
                ("RIG_ITERM2_STRICT_PATH", "1"),
                ("NO_COLOR", "1"),
            ],
        );
        assert_eq!(out_no_tart.status.code(), Some(1));
        assert!(out_no_tart.stdout.is_empty());
        let err_tart = String::from_utf8(out_no_tart.stderr).expect("utf8");
        assert!(err_tart.contains("tart") && err_tart.contains("\n  \u{26a0}\u{fe0f} "));

        write_executable(&bin_no_tart.join("tart"), "#!/bin/sh\nexit 0\n");
        let out_no_vncdo = run_iterm2_script(
            &["capability_probe", "batch"],
            &temp.path,
            &[
                ("PATH", bin_no_tart.to_str().expect("utf8")),
                ("HOME", "/nonexistent"),
                ("RIG_ITERM2_STRICT_PATH", "1"),
                ("NO_COLOR", "1"),
            ],
        );
        assert_eq!(out_no_vncdo.status.code(), Some(1));
        let err_vncdo = String::from_utf8(out_no_vncdo.stderr).expect("utf8");
        assert!(err_vncdo.contains("vncdo") && err_vncdo.contains("\n  \u{26a0}\u{fe0f} "));
    }

    fn setup_mock_harness(temp: &TempTestDir) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let bin_dir = temp.path.join("mock-bin");
        create_hermetic_bin(&bin_dir, &["env", "bash", "uname", "python3"]);
        write_executable(&bin_dir.join("tart"), MOCK_TART_PY);
        write_executable(&bin_dir.join("vncdo"), MOCK_VNCDO_SH);
        let stub_prog = temp.path.join("stub_probe");
        write_executable(&stub_prog, "#!/bin/sh\nexit 0\n");
        let calls_file = temp.path.join("calls.tsv");
        let stop_flag = temp.path.join("stopped.flag");
        (bin_dir, stub_prog, calls_file, stop_flag)
    }

    #[test]
    fn mock_tart_and_vncdo_execute_batch_and_stop_vm_cleanly() {
        let temp = TempTestDir::create("mock-ok");
        let (bin_dir, stub_prog, calls_file, stop_flag) = setup_mock_harness(&temp);
        let shared_ptr = temp.path.join("shared.ptr");
        let out = run_iterm2_script(
            &["capability_probe", "batch"],
            &temp.path,
            &[
                ("PATH", bin_dir.to_str().expect("utf8")),
                ("RIG_ITERM2_STRICT_PATH", "1"),
                ("RIG_ITERM2_PROGRAM_BIN", stub_prog.to_str().expect("utf8")),
                ("RIG_ITERM2_MOCK_GUEST", "1"),
                ("MOCK_TART_CALLS_FILE", calls_file.to_str().expect("utf8")),
                ("MOCK_TART_STOP_FLAG", stop_flag.to_str().expect("utf8")),
                ("MOCK_TART_SHARED_PTR", shared_ptr.to_str().expect("utf8")),
            ],
        );
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout_text = String::from_utf8(out.stdout).expect("utf8");
        assert!(
            stdout_text.contains("\"layer\": 6") && stdout_text.contains("\"scenario\": \"batch\"")
        );
        assert!(String::from_utf8_lossy(&out.stderr).contains("\u{2705}"));
        let calls = fs::read_to_string(&calls_file).expect("calls");
        assert!(calls.contains("TART\tstop\ttuikit-macos"));
    }

    #[test]
    fn failed_step_captures_failure_screenshot_and_stops_vm() {
        let temp = TempTestDir::create("mock-fail");
        let (bin_dir, stub_prog, calls_file, stop_flag) = setup_mock_harness(&temp);
        let shared_ptr = temp.path.join("shared.ptr");
        let log_dir = temp.path.join("artifacts");
        let out = run_iterm2_script(
            &["capability_probe", "batch", "--timeout-ms", "400"],
            &temp.path,
            &[
                ("PATH", bin_dir.to_str().expect("utf8")),
                ("RIG_ITERM2_STRICT_PATH", "1"),
                ("RIG_ITERM2_PROGRAM_BIN", stub_prog.to_str().expect("utf8")),
                ("RIG_ITERM2_MOCK_GUEST", "1"),
                ("RIG_ITERM2_LOG_DIR", log_dir.to_str().expect("utf8")),
                ("MOCK_TART_SKIP_PROBE", "1"),
                ("MOCK_TART_CALLS_FILE", calls_file.to_str().expect("utf8")),
                ("MOCK_TART_STOP_FLAG", stop_flag.to_str().expect("utf8")),
                ("MOCK_TART_SHARED_PTR", shared_ptr.to_str().expect("utf8")),
            ],
        );
        assert!(!out.status.success() && out.stdout.is_empty());
        assert!(log_dir.join("failure-screenshot.png").is_file());
        let calls = fs::read_to_string(&calls_file).expect("calls");
        assert!(calls.contains("TART\tstop\ttuikit-macos"));
    }
}
