"""CLI entrypoint (`python3 -m driver.main`) for Layer 6 (`rig/iterm2/run.sh`)."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys

from driver import DEFAULT_VM_NAME
from driver.cli_args import (
    CliUsageError,
    DriverError,
    DriverRuntimeError,
    ParsedCliArgs,
    build_help_text,
    create_cli_theme,
    parse_cli_args,
)
from driver.iterm_window import backup_guest_defaults, restore_guest_defaults
from driver.matrix_probes import run_iterm2_matrix
from driver.scenarios import ScenarioRuntimeContext, run_named_scenario
from driver.tart_vm import GUEST_SHARED_ROOT, TartVmSession, resolve_host_binary
from driver.vnc_input import VncController

# Directory containing the `driver` Python package.
DRIVER_DIRECTORY = Path(__file__).resolve().parent

# Repository root resolved two levels above `rig/iterm2/driver`.
REPOSITORY_ROOT = DRIVER_DIRECTORY.parents[2]


def log_progress(theme: Mapping[str, str], message: str) -> None:
    """Writes a progress status line to stderr."""
    sys.stderr.write(f"{theme['accent']}==>{theme['reset']} {message}\n")
    sys.stderr.flush()


def log_success(theme: Mapping[str, str], message: str) -> None:
    """Writes a completion summary line to stderr."""
    sys.stderr.write(f"{theme['success']}\u2705 {message}{theme['reset']}\n")
    sys.stderr.flush()


def log_error_with_hint(theme: Mapping[str, str], message: str, recovery_hint: str) -> None:
    """Writes a structured error and indented recovery hint to stderr."""
    sys.stderr.write(f"{theme['error']}\u274c run.sh: {message}{theme['reset']}\n")
    sys.stderr.write(f"  {theme['warn']}\u26a0\ufe0f {recovery_hint}{theme['reset']}\n")
    sys.stderr.flush()


def _build_workspace_example(
    program_name: str, environment: Mapping[str, str], theme: Mapping[str, str]
) -> Path:
    cargo_binary = resolve_host_binary("cargo", environment)
    manifest_path = REPOSITORY_ROOT / "Cargo.toml"
    if not manifest_path.is_file():
        raise DriverRuntimeError(
            f"Workspace Cargo.toml not found at '{manifest_path}'.",
            "Run this script from an intact avandar-tui-kit checkout.",
        )
    log_progress(theme, f"Building workspace example '{program_name}'...")
    cargo_scope = ["-p", "ratatui-mouse", "--features", "vt100"] if program_name == "mouse_lab" else ["--workspace"]
    try:
        completed = subprocess.run(
            [cargo_binary, "build", "--manifest-path", str(manifest_path), *cargo_scope, "--example", program_name],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            env=dict(environment),
            check=False,
        )
    except OSError as os_error:
        raise DriverRuntimeError(
            f"Failed to execute cargo at '{cargo_binary}': {os_error}",
            "Verify Rust and cargo are installed via rustup.",
        ) from os_error
    if completed.returncode != 0:
        raise DriverRuntimeError(
            f"Cargo failed to build example '{program_name}' (exit {completed.returncode}).",
            "Check available example names under rig/probe/examples/ and fix any compile errors above.",
        )
    target_root = (
        Path(environment["CARGO_TARGET_DIR"]).resolve()
        if environment.get("CARGO_TARGET_DIR")
        else REPOSITORY_ROOT / "target"
    )
    binary_path = target_root / "debug" / "examples" / program_name
    if not binary_path.is_file() or not os.access(binary_path, os.X_OK):
        raise DriverRuntimeError(
            f"Compiled example binary was not found at '{binary_path}'.",
            f"Verify '{program_name}' is declared as a Cargo workspace example.",
        )
    return binary_path


def resolve_or_build_program_binary(
    program_name: str,
    env: Mapping[str, str],
    theme: Mapping[str, str],
) -> Path:
    """Resolves `RIG_ITERM2_PROGRAM_BIN` or builds the workspace example via `cargo build`."""
    override_binary = env.get("RIG_ITERM2_PROGRAM_BIN", "").strip()
    if not override_binary:
        return _build_workspace_example(program_name, env, theme)
    resolved = Path(override_binary).expanduser().resolve()
    if resolved.is_file() and os.access(resolved, os.X_OK):
        return resolved
    raise DriverRuntimeError(
        f"RIG_ITERM2_PROGRAM_BIN does not exist or is not executable: '{override_binary}'.",
        "Unset RIG_ITERM2_PROGRAM_BIN or point it to an executable binary.",
    )


def _stage_guest_binaries_and_configs(
    vm: TartVmSession,
    binary_path: Path,
    program_name: str,
) -> tuple[str, dict[str, str]]:
    vm.stage_host_file(binary_path, program_name, is_executable=True)
    for config_name in ("stock", "full"):
        config_source = REPOSITORY_ROOT / "rig" / "tmux" / f"{config_name}.conf"
        if config_source.is_file():
            vm.stage_host_file(config_source, f"tmux_{config_name}.conf", is_executable=False)
    guest_binary = f"/tmp/{program_name}"
    vm.exec_guest(f'cp "{GUEST_SHARED_ROOT}/{program_name}" {guest_binary} && chmod +x {guest_binary}')
    return guest_binary, {
        "stock": f"{GUEST_SHARED_ROOT}/tmux_stock.conf",
        "full": f"{GUEST_SHARED_ROOT}/tmux_full.conf",
    }


def _install_lab_signal_handlers(program):
    """Ensures lab interrupts unwind guest defaults before Tart stops the VM."""
    previous = {}
    if program != "mouse_lab":
        return previous
    def interrupt_lab(number, _frame):
        raise SystemExit(128 + number)
    for number in (signal.SIGINT, signal.SIGTERM):
        previous[number] = signal.getsignal(number)
        signal.signal(number, interrupt_lab)
    return previous


def _run_booted_vm_payload(
    vm: TartVmSession,
    vnc: VncController,
    parsed: ParsedCliArgs,
    binary_path: Path,
    env: Mapping[str, str],
    theme: Mapping[str, str],
) -> dict[str, object]:
    assert parsed.program is not None and parsed.scenario is not None
    artifact_directory = Path(env["RIG_ITERM2_LOG_DIR"]).resolve() if env.get("RIG_ITERM2_LOG_DIR") else None
    guest_binary, guest_configs = _stage_guest_binaries_and_configs(vm, binary_path, parsed.program)
    backup = backup_guest_defaults(vm)
    context = ScenarioRuntimeContext(
        vm=vm,
        vnc=vnc,
        program=parsed.program,
        guest_program_path=guest_binary,
        tmux_mode=parsed.tmux_mode,
        guest_tmux_stocks=guest_configs,
        timeout_ms=parsed.timeout_ms,
        extra_args=parsed.extra_args,
        artifact_dir=artifact_directory,
        is_initial_natural_scrolling=backup.is_natural_swipe_direction,
    )
    is_defaults_restored = False
    previous_handlers = _install_lab_signal_handlers(parsed.program)
    def _restore_defaults_once() -> None:
        nonlocal is_defaults_restored
        if not is_defaults_restored:
            restore_guest_defaults(vm, backup)
            is_defaults_restored = True

    try:
        log_progress(
            theme, f"Running Layer 6 scenario '{parsed.scenario}' (tmux={parsed.tmux_mode or 'none'})..."
        )
        if parsed.scenario == "matrix":
            return run_iterm2_matrix(context, restore_callback=_restore_defaults_once)
        return run_named_scenario(parsed.scenario, context)
    finally:
        try:
            _restore_defaults_once()
        finally:
            for number, previous in previous_handlers.items():
                signal.signal(number, previous)


def _execute_with_vm(
    parsed: ParsedCliArgs,
    tart_binary: str,
    vncdo_binary: str,
    binary_path: Path,
    env: Mapping[str, str],
    theme: Mapping[str, str],
) -> dict[str, object]:
    assert parsed.program is not None and parsed.scenario is not None
    vm_name = env.get("RIG_ITERM2_VM_NAME", DEFAULT_VM_NAME)
    vm = TartVmSession(tart_bin=tart_binary, vm_name=vm_name, env=env)
    log_progress(theme, f"Cold-booting headless Tart VM '{vm_name}' with loopback VNC...")
    try:
        endpoint = vm.boot(timeout_seconds=max(parsed.timeout_ms / 1000.0, 60.0))
        vnc = VncController(vncdo_bin=vncdo_binary, endpoint=endpoint, env=env)
        payload = _run_booted_vm_payload(vm, vnc, parsed, binary_path, env, theme)
        return {
            "layer": 6,
            "platform": sys.platform,
            "arch": platform.machine(),
            "vmName": vm_name,
            "program": parsed.program,
            "tmuxConfig": parsed.tmux_mode,
            **payload,
        }
    finally:
        log_progress(theme, f"Stopping Tart VM '{vm_name}'...")
        vm.stop()


def run_cli(argv: Sequence[str], env: Mapping[str, str] | None = None) -> int:
    """Main CLI execution returning process exit status."""
    active_environment = dict(os.environ if env is None else env)
    stdout_theme = create_cli_theme(sys.stdout.isatty(), active_environment)
    stderr_theme = create_cli_theme(sys.stderr.isatty(), active_environment)
    try:
        parsed = parse_cli_args(argv)
        if parsed.is_help:
            sys.stdout.write(f"{build_help_text(stdout_theme)}\n")
            sys.stdout.flush()
            return 0
        assert parsed.program is not None and parsed.scenario is not None
        tart_binary = resolve_host_binary("tart", active_environment)
        vncdo_binary = resolve_host_binary("vncdo", active_environment)
        program_binary = resolve_or_build_program_binary(parsed.program, active_environment, stderr_theme)
        report = _execute_with_vm(
            parsed, tart_binary, vncdo_binary, program_binary, active_environment, stderr_theme
        )
        if active_environment.get("RIG_ITERM2_LOG_DIR"):
            output_directory = Path(active_environment["RIG_ITERM2_LOG_DIR"]).resolve()
            output_directory.mkdir(parents=True, exist_ok=True)
            (output_directory / f"iterm2-{parsed.program}-{parsed.scenario}-report.json").write_text(
                f"{json.dumps(report, indent=2)}\n",
                encoding="utf-8",
            )
        sys.stdout.write(f"{json.dumps(report, indent=2)}\n")
        sys.stdout.flush()
        log_success(stderr_theme, f"Layer 6 '{parsed.scenario}' completed cleanly.")
        return 0
    except CliUsageError as usage_error:
        sys.stderr.write(f"{build_help_text(stderr_theme)}\n\n")
        log_error_with_hint(stderr_theme, usage_error.message, usage_error.recovery_hint)
        return usage_error.exit_code
    except DriverError as driver_error:
        log_error_with_hint(stderr_theme, driver_error.message, driver_error.recovery_hint)
        return driver_error.exit_code


def main() -> None:
    """CLI entrypoint executing Layer 6 and exiting with its status code."""
    raise SystemExit(run_cli(sys.argv[1:]))


if __name__ == "__main__":
    main()
