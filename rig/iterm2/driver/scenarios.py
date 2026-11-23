"""Layer 6 scenario coordinator (`batch`, `osc52`, `interactive`, `panic`, `passthrough`, `all`)."""

from __future__ import annotations

from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
import json
from pathlib import Path
import shutil
import tempfile
from typing import Protocol

from driver.cli_args import DriverError, DriverRuntimeError, merge_probe_args
from driver.interactive_steps import (
    ensure_iterm_window_focused, finish_interactive_clean_exit, prepare_ready_interactive_session,
    run_fenced_vnc_step, run_focus_steps, run_keyboard_steps, run_paste_step,
    run_pointer_steps, wait_for_session_exit_and_teardown,
)
from driver.iterm_window import SessionLaunchSpec, launch_iterm_session, resolve_session_geometry
from driver.tart_vm import TartVmSession
from driver.vnc_input import (
    VncController, WindowGeometry, build_chord_tokens, build_click_tokens, cell_center_to_vnc_pixels,
)
from driver.wait_helpers import read_all_jsonl_records, wait_for_probe_ready


class ScreenshotCapable(Protocol):
    """Protocol for VNC controllers capable of capturing a PNG failure screenshot."""

    def capture_screenshot(self, output_path: Path) -> Path:
        """Captures a screenshot to `output_path` and returns the written path."""
        ...


@dataclass
class ScenarioRuntimeContext:
    """Shared runtime state across scenarios within a booted VM session."""

    vm: TartVmSession
    vnc: VncController
    program: str
    guest_program_path: str
    tmux_mode: str | None
    guest_tmux_stocks: dict[str, str]
    timeout_ms: int
    extra_args: list[str] = field(default_factory=list)
    artifact_dir: Path | None = None
    is_initial_natural_scrolling: bool = True
    is_natural_scrolling: bool = False
    _session_counter: int = 0

    @property
    def timeout_seconds(self) -> float:
        """Returns the per-phase condition timeout converted to seconds."""
        return max(self.timeout_ms / 1000.0, 0.2)

    def next_session_id(self, prefix: str) -> str:
        """Allocates a monotonic session identifier prefixed with `prefix`."""
        self._session_counter += 1
        return f"{prefix}_{self._session_counter:02d}"

    def get_guest_tmux_config_path_from_mode(self, mode: str | None) -> str | None:
        """Resolves the staged guest tmux configuration path for `mode`, or None when direct."""
        if mode is None:
            return None
        profile = mode[3:] if mode.startswith("cc-") else mode
        return self.guest_tmux_stocks[profile]


def capture_failure_screenshot_safe(
    vnc: ScreenshotCapable | None, artifact_dir: Path | None
) -> Path | None:
    """Captures `failure-screenshot.png` via `vnc` without masking the primary exception."""
    if vnc is None or not hasattr(vnc, "capture_screenshot"):
        return None
    created_temp_dir = (
        Path(tempfile.mkdtemp(prefix="tuikit-iterm2-failure-")) if artifact_dir is None else None
    )
    target_dir = created_temp_dir if created_temp_dir is not None else artifact_dir
    assert target_dir is not None
    screenshot_path = target_dir / "failure-screenshot.png"
    try:
        vnc.capture_screenshot(screenshot_path)
        return screenshot_path
    except Exception:  # noqa: BLE001
        if created_temp_dir is not None:
            shutil.rmtree(created_temp_dir, ignore_errors=True)
        return None


def run_with_failure_screenshot(
    context: object, action: Callable[[], dict[str, object]]
) -> dict[str, object]:
    """Runs `action` and captures `failure-screenshot.png` onto `DriverError` hints on failure."""
    try:
        return action()
    except Exception as captured_error:
        screenshot_path = capture_failure_screenshot_safe(
            getattr(context, "vnc", None), getattr(context, "artifact_dir", None)
        )
        if screenshot_path is not None and isinstance(captured_error, DriverError):
            captured_error.recovery_hint = (
                f"{captured_error.recovery_hint} (Screenshot saved to {screenshot_path})"
            )
        raise


def _archive_session_log_if_requested(log_path: Path, artifact_dir: Path | None, label: str) -> None:
    if artifact_dir is None or not log_path.is_file():
        return
    artifact_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(log_path, artifact_dir / f"{label}.jsonl")


def execute_batch_scenario(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs `<program> --batch`, verifies exit 0 and DECRQM mode reset, and returns records."""
    session_id = context.next_session_id("batch")
    spec = SessionLaunchSpec(
        session_id=session_id,
        guest_program_path=context.guest_program_path,
        program_args=merge_probe_args(["--batch"], context.extra_args),
        tmux_mode=context.tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(context.tmux_mode),
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, context.timeout_seconds, session.host_exit_path)
        exit_outcome = wait_for_session_exit_and_teardown(session, context.timeout_seconds)
        if exit_outcome["exitCode"] != 0:
            raise DriverRuntimeError(
                f"batch probe exited with code {exit_outcome['exitCode']}.",
                "Inspect captured probe JSONL records for error details.",
            )
        return {
            "scenario": "batch",
            "exitCode": exit_outcome["exitCode"],
            "terminalStateAfterExit": exit_outcome["terminalStateAfterExit"],
            "records": read_all_jsonl_records(session.host_log_path),
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)


def _run_single_osc52_probe(
    context: ScenarioRuntimeContext,
    form: str,
    token: str,
    tmux_mode: str | None,
    should_allow_clipboard_access: bool = False,
) -> dict[str, object]:
    context.vm.write_guest_clipboard("TUIKIT_OSC52_UNTOUCHED")
    session_id = context.next_session_id(f"osc52_{form}")
    spec = SessionLaunchSpec(
        session_id=session_id,
        guest_program_path=context.guest_program_path,
        program_args=merge_probe_args(
            ["--batch", "--osc52", token, "--osc52-form", form], context.extra_args
        ),
        tmux_mode=tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(tmux_mode),
        should_allow_clipboard_access=should_allow_clipboard_access,
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, context.timeout_seconds, session.host_exit_path)
        exit_outcome = wait_for_session_exit_and_teardown(session, context.timeout_seconds)
        if exit_outcome["exitCode"] != 0:
            raise DriverRuntimeError(f"osc52 ({form}) exited with {exit_outcome['exitCode']}.", "Check logs.")
        guest_clipboard = context.vm.read_guest_clipboard()
        return {
            "form": form,
            "token": token,
            "allowClipboardAccess": should_allow_clipboard_access,
            "guestClipboard": guest_clipboard,
            "clipboardUpdated": guest_clipboard == token,
            "tmuxBuffer": exit_outcome["tmuxBuffer"],
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)


def execute_osc52_scenario(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures OSC 52 clipboard and tmux paste-buffer behavior for the active configuration."""
    specs = (
        [("plain", "tuikit-iterm2-osc52-plain")]
        if context.tmux_mode is None
        else [
            ("plain", "tuikit-tmux-osc52-plain"),
            ("tmux", "tuikit-tmux-osc52-wrapped"),
            ("both", "tuikit-tmux-osc52-both"),
        ]
    )
    return {
        "scenario": "osc52",
        "probes": [_run_single_osc52_probe(context, form, token, context.tmux_mode, False) for form, token in specs],
    }


def _run_kitty_enter_subrun(context: ScenarioRuntimeContext) -> dict[str, list[dict[str, object]]]:
    modifiers = context.vnc.calibrate_modifiers(context.vm)
    session_id = context.next_session_id("interactive_kitty")
    spec = SessionLaunchSpec(
        session_id=session_id,
        guest_program_path=context.guest_program_path,
        program_args=merge_probe_args(["--push-kitty"], context.extra_args),
        tmux_mode=context.tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(context.tmux_mode),
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        prepare_ready_interactive_session(
            context.vm, context.vnc, session, "sync_kitty_ready", context.timeout_seconds
        )
        kitty_steps: dict[str, list[dict[str, object]]] = {}
        for label, modifier_name in (
            ("alt_enter", modifiers.option_key),
            ("shift_enter", modifiers.shift_key),
            ("ctrl_enter", modifiers.ctrl_key),
        ):
            chord_tokens = build_chord_tokens("enter", (modifier_name,))
            kitty_steps[label] = run_fenced_vnc_step(
                context.vm, context.vnc, session, f"kitty_{label}",
                lambda tokens=chord_tokens: context.vnc.run_tokens(tokens), context.timeout_seconds,
            )
        finish_interactive_clean_exit(context.vnc, session, context.timeout_seconds)
        return kitty_steps
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)


def _format_window_geometry(geometry: WindowGeometry) -> dict[str, object]:
    return {
        "windowXPt": geometry.window_x_pt, "windowYPt": geometry.window_y_pt,
        "windowWidthPt": geometry.window_width_pt, "windowHeightPt": geometry.window_height_pt,
        "cols": geometry.cols, "rows": geometry.rows,
        "cellWidthPt": geometry.cell_width_pt, "cellHeightPt": geometry.cell_height_pt,
    }


def execute_interactive_scenario(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Drives pointer, keyboard, paste, focus, and Kitty Enter subruns over VNC."""
    modifiers = context.vnc.calibrate_modifiers(context.vm)
    session_id = context.next_session_id("interactive")
    spec = SessionLaunchSpec(
        session_id, context.guest_program_path, merge_probe_args([], context.extra_args),
        tmux_mode=context.tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(context.tmux_mode),
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        geometry = prepare_ready_interactive_session(
            context.vm, context.vnc, session, "sync_ready", context.timeout_seconds
        )
        metadata_path = session.host_meta_path
        session_metadata = (
            json.loads(metadata_path.read_text(encoding="utf-8")) if metadata_path.is_file() else {}
        )
        pointer_steps = run_pointer_steps(context.vm, context.vnc, session, geometry, modifiers, context.timeout_seconds)
        keyboard_steps = run_keyboard_steps(context.vm, context.vnc, session, modifiers, context.timeout_seconds)
        paste_events = run_paste_step(context.vm, context.vnc, session, modifiers, context.timeout_seconds)
        focus = run_focus_steps(context.vm, context.vnc, session, geometry, context.timeout_seconds)
        clean_exit = finish_interactive_clean_exit(context.vnc, session, context.timeout_seconds)
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)
    kitty_enter_steps = _run_kitty_enter_subrun(context)
    tmux_version = session_metadata.get("tmuxVersion")
    return {
        "scenario": "interactive",
        "calibratedModifiers": {"optionKey": modifiers.option_key, "commandKey": modifiers.command_key},
        "windowGeometry": _format_window_geometry(geometry),
        "tmuxServerInfo": {"version": tmux_version} if tmux_version else None,
        "pointerSteps": pointer_steps,
        "keyboardSteps": keyboard_steps,
        "pasteEvents": paste_events,
        "focus": focus,
        "kittyEnterSteps": kitty_enter_steps,
        "exitCode": clean_exit["exitCode"],
        "exitReason": "clean_key_exit",
        "terminalStateAfterExit": clean_exit["terminalStateAfterExit"],
    }


def execute_panic_scenario(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Triggers the F12 test panic over VNC and verifies non-zero exit + DECRQM mode teardown."""
    session_id = context.next_session_id("panic")
    spec = SessionLaunchSpec(
        session_id=session_id,
        guest_program_path=context.guest_program_path,
        program_args=merge_probe_args([], context.extra_args),
        tmux_mode=context.tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(context.tmux_mode),
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, context.timeout_seconds, session.host_exit_path)
        geometry = resolve_session_geometry(context.vm, session, context.timeout_seconds)
        ensure_iterm_window_focused(context.vm, context.vnc, geometry, context.timeout_seconds)
        click_x, click_y = cell_center_to_vnc_pixels(geometry, 10, 5)
        pre_click = run_fenced_vnc_step(
            context.vm, context.vnc, session, "pre_panic_click",
            lambda: context.vnc.run_tokens(build_click_tokens(click_x, click_y, 1)), context.timeout_seconds,
        )
        if not any(record.get("event_type") == "mouse" for record in pre_click):
            raise DriverRuntimeError("Expected active mouse tracking before F12 panic.", "Check M1 setup.")
        context.vnc.run_tokens(["key", "f12"])
        exit_outcome = wait_for_session_exit_and_teardown(session, context.timeout_seconds)
        if exit_outcome["exitCode"] == 0 and exit_outcome["signal"] == 0:
            raise DriverRuntimeError("Expected non-zero exit code when F12 panic triggers.", "Check F12 hook.")
        return {
            "scenario": "panic",
            "exitCode": exit_outcome["exitCode"],
            "signal": exit_outcome["signal"],
            "terminalStateAfterPanic": exit_outcome["terminalStateAfterExit"],
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)


def execute_passthrough_scenario(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs a custom `<program>` with `--log` managed by the driver and returns its JSONL log."""
    session_id = context.next_session_id("passthrough")
    spec = SessionLaunchSpec(
        session_id=session_id,
        guest_program_path=context.guest_program_path,
        program_args=list(context.extra_args),
        tmux_mode=context.tmux_mode,
        guest_tmux_conf_path=context.get_guest_tmux_config_path_from_mode(context.tmux_mode),
    )
    session = launch_iterm_session(context.vm, spec)
    try:
        exit_outcome = wait_for_session_exit_and_teardown(session, context.timeout_seconds)
        if exit_outcome["exitCode"] != 0:
            raise DriverRuntimeError(
                f"passthrough program exited with code {exit_outcome['exitCode']}.",
                "Inspect program stderr or JSONL output.",
            )
        return {
            "scenario": "passthrough",
            "exitCode": exit_outcome["exitCode"],
            "records": read_all_jsonl_records(session.host_log_path),
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, context.artifact_dir, session_id)


def execute_all_scenarios(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs `batch`, `osc52`, `interactive`, and `panic` inside one booted VM."""
    return {
        "scenario": "all",
        "batch": execute_batch_scenario(context),
        "osc52": execute_osc52_scenario(context),
        "interactive": execute_interactive_scenario(context),
        "panic": execute_panic_scenario(context),
    }


# Dispatch table mapping non-matrix scenario names to their execution functions.
DEFAULT_SCENARIO_TABLE: dict[str, Callable[[ScenarioRuntimeContext], dict[str, object]]] = {
    "batch": execute_batch_scenario,
    "osc52": execute_osc52_scenario,
    "interactive": execute_interactive_scenario,
    "panic": execute_panic_scenario,
    "passthrough": execute_passthrough_scenario,
    "all": execute_all_scenarios,
}


def run_named_scenario(
    scenario_name: str,
    context: ScenarioRuntimeContext,
    scenario_overrides: Mapping[str, Callable[[ScenarioRuntimeContext], dict[str, object]]] | None = None,
) -> dict[str, object]:
    """Dispatches `scenario_name` and automatically captures `failure-screenshot.png` on failure."""
    if scenario_name == "mouse" or scenario_name.startswith("x"):
        from driver.mouse_scenarios import run_mouse_scenario
        return run_with_failure_screenshot(context, lambda: run_mouse_scenario(context, scenario_name))
    table = dict(DEFAULT_SCENARIO_TABLE)
    if scenario_overrides:
        table.update(scenario_overrides)
    runner = table.get(scenario_name)
    if runner is None:
        raise DriverRuntimeError(f"Unsupported scenario '{scenario_name}'.", "Pass --help for valid scenarios.")
    return run_with_failure_screenshot(context, lambda: runner(context))
