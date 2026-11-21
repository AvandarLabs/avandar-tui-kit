"""CLI entrypoint (`python3 -m driver.main`) for Layer 6 (`rig/iterm2/run.sh`)."""

from __future__ import annotations

import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from typing import Mapping, Sequence

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

DRIVER_DIR = Path(__file__).resolve().parent
REPOSITORY_ROOT = DRIVER_DIR.parents[2]


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


def _build_workspace_example(program_name: str, env: Mapping[str, str], theme: Mapping[str, str]) -> Path:
    cargo_bin = resolve_host_binary("cargo", env)
    manifest_path = REPOSITORY_ROOT / "Cargo.toml"
    if not manifest_path.is_file():
        raise DriverRuntimeError(
            f"Workspace Cargo.toml not found at '{manifest_path}'.",
            "Run this script from an intact avandar-tui-kit checkout.",
        )
    log_progress(theme, f"Building workspace example '{program_name}'...")
    proc = subprocess.run(
        [cargo_bin, "build", "--manifest-path", str(manifest_path), "--workspace", "--example", program_name],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        env=dict(env),
        check=False,
    )
    if proc.returncode != 0:
        raise DriverRuntimeError(
            f"Cargo failed to build example '{program_name}' (exit {proc.returncode}).",
            "Check available example names under rig/probe/examples/ and fix any compile errors above.",
        )
    target_root = Path(env["CARGO_TARGET_DIR"]).resolve() if env.get("CARGO_TARGET_DIR") else REPOSITORY_ROOT / "target"
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
    override_bin = env.get("RIG_ITERM2_PROGRAM_BIN", "").strip()
    if not override_bin:
        return _build_workspace_example(program_name, env, theme)
    resolved = Path(override_bin).expanduser().resolve()
    if resolved.is_file() and os.access(resolved, os.X_OK):
        return resolved
    raise DriverRuntimeError(
        f"RIG_ITERM2_PROGRAM_BIN does not exist or is not executable: '{override_bin}'.",
        "Unset RIG_ITERM2_PROGRAM_BIN or point it to an executable binary.",
    )


def _stage_guest_binaries_and_configs(
    vm: TartVmSession,
    binary_path: Path,
    program_name: str,
) -> tuple[str, dict[str, str]]:
    vm.stage_host_file(binary_path, program_name, executable=True)
    for conf_name in ("stock", "full"):
        conf_src = REPOSITORY_ROOT / "rig" / "tmux" / f"{conf_name}.conf"
        if conf_src.is_file():
            vm.stage_host_file(conf_src, f"tmux_{conf_name}.conf", executable=False)
    guest_bin = f"/tmp/{program_name}"
    if not vm.env.get("RIG_ITERM2_MOCK_GUEST"):
        vm.exec_guest(f'cp "{GUEST_SHARED_ROOT}/{program_name}" {guest_bin} && chmod +x {guest_bin}')
    return guest_bin, {
        "stock": f"{GUEST_SHARED_ROOT}/tmux_stock.conf",
        "full": f"{GUEST_SHARED_ROOT}/tmux_full.conf",
    }


def _run_booted_vm_payload(
    vm: TartVmSession,
    vnc: VncController,
    parsed: ParsedCliArgs,
    binary_path: Path,
    env: Mapping[str, str],
    theme: Mapping[str, str],
) -> dict[str, object]:
    assert parsed.program is not None and parsed.scenario is not None
    artifact_dir = Path(env["RIG_ITERM2_LOG_DIR"]).resolve() if env.get("RIG_ITERM2_LOG_DIR") else None
    guest_bin, guest_confs = _stage_guest_binaries_and_configs(vm, binary_path, parsed.program)
    backup = None if env.get("RIG_ITERM2_MOCK_GUEST") else backup_guest_defaults(vm)
    ctx = ScenarioRuntimeContext(
        vm=vm,
        vnc=vnc,
        program=parsed.program,
        guest_program_path=guest_bin,
        tmux_mode=parsed.tmux_mode,
        guest_tmux_stocks=guest_confs,
        timeout_ms=parsed.timeout_ms,
        extra_args=parsed.extra_args,
        artifact_dir=artifact_dir,
    )
    try:
        log_progress(theme, f"Running Layer 6 scenario '{parsed.scenario}' (tmux={parsed.tmux_mode or 'none'})...")
        if parsed.scenario == "matrix":
            return run_iterm2_matrix(
                ctx,
                restore_callback=lambda: restore_guest_defaults(vm, backup) if backup else None,
            )
        return run_named_scenario(parsed.scenario, ctx)
    finally:
        if backup is not None:
            restore_guest_defaults(vm, backup)


def _execute_with_vm(
    parsed: ParsedCliArgs,
    tart_bin: str,
    vncdo_bin: str,
    binary_path: Path,
    env: Mapping[str, str],
    theme: Mapping[str, str],
) -> dict[str, object]:
    assert parsed.program is not None and parsed.scenario is not None
    vm_name = env.get("RIG_ITERM2_VM_NAME", DEFAULT_VM_NAME)
    vm = TartVmSession(tart_bin=tart_bin, vm_name=vm_name, env=env)
    log_progress(theme, f"Cold-booting headless Tart VM '{vm_name}' with loopback VNC...")
    try:
        endpoint = vm.boot(timeout_seconds=max(parsed.timeout_ms / 1000.0, 60.0))
        vnc = VncController(vncdo_bin=vncdo_bin, endpoint=endpoint, env=env)
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
    active_env = dict(os.environ if env is None else env)
    stdout_theme = create_cli_theme(sys.stdout.isatty(), active_env)
    stderr_theme = create_cli_theme(sys.stderr.isatty(), active_env)
    try:
        parsed = parse_cli_args(argv)
        if parsed.is_help:
            sys.stdout.write(f"{build_help_text(stdout_theme)}\n")
            sys.stdout.flush()
            return 0
        assert parsed.program is not None and parsed.scenario is not None
        tart_bin = resolve_host_binary("tart", active_env)
        vncdo_bin = resolve_host_binary("vncdo", active_env)
        binary_path = resolve_or_build_program_binary(parsed.program, active_env, stderr_theme)
        report = _execute_with_vm(parsed, tart_bin, vncdo_bin, binary_path, active_env, stderr_theme)
        if active_env.get("RIG_ITERM2_LOG_DIR"):
            out_dir = Path(active_env["RIG_ITERM2_LOG_DIR"]).resolve()
            out_dir.mkdir(parents=True, exist_ok=True)
            (out_dir / f"iterm2-{parsed.program}-{parsed.scenario}-report.json").write_text(
                f"{json.dumps(report, indent=2)}\n",
                encoding="utf-8",
            )
        sys.stdout.write(f"{json.dumps(report, indent=2)}\n")
        sys.stdout.flush()
        log_success(stderr_theme, f"Layer 6 '{parsed.scenario}' completed cleanly.")
        return 0
    except CliUsageError as usage_err:
        sys.stderr.write(f"{build_help_text(stderr_theme)}\n\n")
        log_error_with_hint(stderr_theme, usage_err.message, usage_err.recovery_hint)
        return usage_err.exit_code
    except DriverError as driver_err:
        log_error_with_hint(stderr_theme, driver_err.message, driver_err.recovery_hint)
        return driver_err.exit_code


def main() -> None:
    raise SystemExit(run_cli(sys.argv[1:]))


if __name__ == "__main__":
    main()
