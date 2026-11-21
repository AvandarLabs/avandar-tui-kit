"""Layer 6 scenario coordinator (`batch`, `osc52`, `interactive`, `panic`, `passthrough`, `all`)."""

from __future__ import annotations

from dataclasses import dataclass, field
import json
from pathlib import Path
import shutil
import tempfile
from typing import Callable, Mapping

from driver.cli_args import DriverError, DriverRuntimeError, merge_probe_args
from driver.interactive_steps import (
    ensure_iterm_window_focused,
    finish_interactive_clean_exit,
    prepare_ready_interactive_session,
    run_fenced_vnc_step,
    run_focus_steps,
    run_keyboard_steps,
    run_paste_step,
    run_pointer_steps,
    wait_for_session_exit_and_teardown,
)
from driver.iterm_window import (
    SessionLaunchSpec,
    launch_iterm_session,
    resolve_session_geometry,
)
from driver.tart_vm import TartVmSession
from driver.vnc_input import VncController, build_chord_tokens, build_click_tokens, cell_center_to_vnc_pixels
from driver.wait_helpers import read_all_jsonl_records, wait_for_probe_ready


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
    _counter: int = 0

    @property
    def timeout_seconds(self) -> float:
        return max(self.timeout_ms / 1000.0, 0.2)

    def next_session_id(self, prefix: str) -> str:
        self._counter += 1
        return f"{prefix}_{self._counter:02d}"

    def guest_tmux_conf_for(self, mode: str | None) -> str | None:
        if mode is None:
            return None
        profile = mode[3:] if mode.startswith("cc-") else mode
        return self.guest_tmux_stocks[profile]


def capture_failure_screenshot_safe(vnc: object, artifact_dir: Path | None) -> Path | None:
    """Captures `failure-screenshot.png` via `vnc` without masking the primary exception."""
    if vnc is None or not hasattr(vnc, "capture_screenshot"):
        return None
    target_dir = (
        artifact_dir
        if artifact_dir is not None
        else Path(tempfile.mkdtemp(prefix="tuikit-iterm2-failure-"))
    )
    screenshot_path = target_dir / "failure-screenshot.png"
    try:
        vnc.capture_screenshot(screenshot_path)  # type: ignore[attr-defined]
        return screenshot_path
    except Exception:  # noqa: BLE001
        return None


def _archive_session_log_if_requested(log_path: Path, artifact_dir: Path | None, label: str) -> None:
    if artifact_dir is None or not log_path.is_file():
        return
    artifact_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(log_path, artifact_dir / f"{label}.jsonl")


def execute_batch_scenario(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs `<program> --batch`, verifies exit 0 and DECRQM mode reset, and returns records."""
    sid = ctx.next_session_id("batch")
    spec = SessionLaunchSpec(
        session_id=sid,
        guest_program_path=ctx.guest_program_path,
        program_args=merge_probe_args(["--batch"], ctx.extra_args),
        tmux_mode=ctx.tmux_mode,
        guest_tmux_conf_path=ctx.guest_tmux_conf_for(ctx.tmux_mode),
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, ctx.timeout_seconds, session.host_exit_path)
        exit_outcome = wait_for_session_exit_and_teardown(session, ctx.timeout_seconds)
        if exit_outcome["exitCode"] != 0:
            raise DriverRuntimeError(
                f"batch probe exited with code {exit_outcome['exitCode']}.",
                "Inspect captured probe JSONL records for error details.",
            )
        records = read_all_jsonl_records(session.host_log_path)
        return {
            "scenario": "batch",
            "exitCode": exit_outcome["exitCode"],
            "terminalStateAfterExit": exit_outcome["terminalStateAfterExit"],
            "records": records,
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)


def _run_single_osc52_probe(
    ctx: ScenarioRuntimeContext,
    form: str,
    token: str,
    tmux_mode: str | None,
    allow_clipboard_access: bool = False,
) -> dict[str, object]:
    sentinel = "TUIKIT_OSC52_UNTOUCHED"
    ctx.vm.write_guest_clipboard(sentinel)
    sid = ctx.next_session_id(f"osc52_{form}")
    spec = SessionLaunchSpec(
        session_id=sid,
        guest_program_path=ctx.guest_program_path,
        program_args=merge_probe_args(["--batch", "--osc52", token, "--osc52-form", form], ctx.extra_args),
        tmux_mode=tmux_mode,
        guest_tmux_conf_path=ctx.guest_tmux_conf_for(tmux_mode),
        allow_clipboard_access=allow_clipboard_access,
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, ctx.timeout_seconds, session.host_exit_path)
        exit_outcome = wait_for_session_exit_and_teardown(session, ctx.timeout_seconds)
        if exit_outcome["exitCode"] != 0:
            raise DriverRuntimeError(f"osc52 ({form}) exited with {exit_outcome['exitCode']}.", "Check logs.")
        guest_clip = ctx.vm.read_guest_clipboard()
        return {
            "form": form,
            "token": token,
            "allowClipboardAccess": allow_clipboard_access,
            "guestClipboard": guest_clip,
            "clipboardUpdated": guest_clip == token,
            "tmuxBuffer": exit_outcome["tmuxBuffer"],
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)


def execute_osc52_scenario(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures OSC 52 clipboard and tmux paste-buffer behavior for the active configuration."""
    specs = (
        [("plain", "tuikit-iterm2-osc52-plain")]
        if ctx.tmux_mode is None
        else [
            ("plain", "tuikit-tmux-osc52-plain"),
            ("tmux", "tuikit-tmux-osc52-wrapped"),
            ("both", "tuikit-tmux-osc52-both"),
        ]
    )
    probes = [_run_single_osc52_probe(ctx, form, tok, ctx.tmux_mode, False) for form, tok in specs]
    return {"scenario": "osc52", "probes": probes}


def _run_kitty_enter_subrun(ctx: ScenarioRuntimeContext) -> dict[str, list[dict[str, object]]]:
    mods = ctx.vnc.calibrate_modifiers(ctx.vm)
    sid = ctx.next_session_id("interactive_kitty")
    spec = SessionLaunchSpec(
        session_id=sid,
        guest_program_path=ctx.guest_program_path,
        program_args=merge_probe_args(["--push-kitty"], ctx.extra_args),
        tmux_mode=ctx.tmux_mode,
        guest_tmux_conf_path=ctx.guest_tmux_conf_for(ctx.tmux_mode),
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        prepare_ready_interactive_session(ctx.vm, ctx.vnc, session, "sync_kitty_ready", ctx.timeout_seconds)
        kitty_steps: dict[str, list[dict[str, object]]] = {}
        for label, mod_name in (
            ("alt_enter", mods.option_key),
            ("shift_enter", mods.shift_key),
            ("ctrl_enter", mods.ctrl_key),
        ):
            kitty_steps[label] = run_fenced_vnc_step(
                ctx.vm,
                ctx.vnc,
                session,
                f"kitty_{label}",
                lambda m=mod_name: ctx.vnc.run_tokens(build_chord_tokens("enter", (m,))),
                ctx.timeout_seconds,
            )
        finish_interactive_clean_exit(ctx.vnc, session, ctx.timeout_seconds)
        return kitty_steps
    finally:
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)


def execute_interactive_scenario(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Drives pointer, keyboard, paste, focus, and Kitty Enter subruns over VNC."""
    mods = ctx.vnc.calibrate_modifiers(ctx.vm)
    sid = ctx.next_session_id("interactive")
    spec = SessionLaunchSpec(
        sid, ctx.guest_program_path, merge_probe_args([], ctx.extra_args),
        tmux_mode=ctx.tmux_mode, guest_tmux_conf_path=ctx.guest_tmux_conf_for(ctx.tmux_mode),
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        geom = prepare_ready_interactive_session(ctx.vm, ctx.vnc, session, "sync_ready", ctx.timeout_seconds)
        meta_path = session.host_meta_path
        meta = json.loads(meta_path.read_text(encoding="utf-8")) if meta_path.is_file() else {}
        pointer_steps = run_pointer_steps(ctx.vm, ctx.vnc, session, geom, mods, ctx.timeout_seconds)
        keyboard_steps = run_keyboard_steps(ctx.vm, ctx.vnc, session, mods, ctx.timeout_seconds)
        paste_events = run_paste_step(ctx.vm, ctx.vnc, session, mods, ctx.timeout_seconds)
        focus = run_focus_steps(ctx.vm, ctx.vnc, session, geom, ctx.timeout_seconds)
        clean_exit = finish_interactive_clean_exit(ctx.vnc, session, ctx.timeout_seconds)
    finally:
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)
    kitty_enter_steps = _run_kitty_enter_subrun(ctx)
    return {
        "scenario": "interactive",
        "calibratedModifiers": {"optionKey": mods.option_key, "commandKey": mods.command_key},
        "windowGeometry": {
            "windowXPt": geom.window_x_pt, "windowYPt": geom.window_y_pt,
            "windowWidthPt": geom.window_width_pt, "windowHeightPt": geom.window_height_pt,
            "cols": geom.cols, "rows": geom.rows,
            "cellWidthPt": geom.cell_width_pt, "cellHeightPt": geom.cell_height_pt,
        },
        "tmuxServerInfo": {"version": meta.get("tmuxVersion")} if meta.get("tmuxVersion") else None,
        "pointerSteps": pointer_steps,
        "keyboardSteps": keyboard_steps,
        "pasteEvents": paste_events,
        "focus": focus,
        "kittyEnterSteps": kitty_enter_steps,
        "exitCode": clean_exit["exitCode"],
        "exitReason": "clean_key_exit",
        "terminalStateAfterExit": clean_exit["terminalStateAfterExit"],
    }


def execute_panic_scenario(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Triggers the F12 test panic over VNC and verifies non-zero exit + DECRQM mode teardown."""
    sid = ctx.next_session_id("panic")
    spec = SessionLaunchSpec(
        session_id=sid,
        guest_program_path=ctx.guest_program_path,
        program_args=merge_probe_args([], ctx.extra_args),
        tmux_mode=ctx.tmux_mode,
        guest_tmux_conf_path=ctx.guest_tmux_conf_for(ctx.tmux_mode),
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        wait_for_probe_ready(session.host_log_path, ctx.timeout_seconds, session.host_exit_path)
        geom = resolve_session_geometry(ctx.vm, session, ctx.timeout_seconds)
        ensure_iterm_window_focused(ctx.vm, ctx.vnc, geom, ctx.timeout_seconds)
        if ctx.tmux_mode in (None, "full", "cc-full"):
            cx, cy = cell_center_to_vnc_pixels(geom, 10, 5)
            pre_click = run_fenced_vnc_step(
                ctx.vm,
                ctx.vnc,
                session,
                "pre_panic_click",
                lambda: ctx.vnc.run_tokens(build_click_tokens(cx, cy, 1)),
                ctx.timeout_seconds,
            )
            if not any(r.get("event_type") == "mouse" for r in pre_click):
                raise DriverRuntimeError("Expected active mouse tracking before F12 panic.", "Check M1 setup.")
        ctx.vnc.run_tokens(["key", "f12"])
        exit_outcome = wait_for_session_exit_and_teardown(session, ctx.timeout_seconds)
        if exit_outcome["exitCode"] == 0 and exit_outcome["signal"] == 0:
            raise DriverRuntimeError("Expected non-zero exit code when F12 panic triggers.", "Check F12 hook.")
        return {
            "scenario": "panic",
            "exitCode": exit_outcome["exitCode"],
            "signal": exit_outcome["signal"],
            "terminalStateAfterPanic": exit_outcome["terminalStateAfterExit"],
        }
    finally:
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)


def execute_passthrough_scenario(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs a custom `<program>` with `--log` managed by the driver and returns its JSONL log."""
    sid = ctx.next_session_id("passthrough")
    spec = SessionLaunchSpec(
        session_id=sid,
        guest_program_path=ctx.guest_program_path,
        program_args=list(ctx.extra_args),
        tmux_mode=ctx.tmux_mode,
        guest_tmux_conf_path=ctx.guest_tmux_conf_for(ctx.tmux_mode),
    )
    session = launch_iterm_session(ctx.vm, spec)
    try:
        exit_outcome = wait_for_session_exit_and_teardown(session, ctx.timeout_seconds)
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
        _archive_session_log_if_requested(session.host_log_path, ctx.artifact_dir, sid)


def execute_all_scenarios(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Runs `batch`, `osc52`, `interactive`, and `panic` inside one booted VM."""
    return {
        "scenario": "all",
        "batch": execute_batch_scenario(ctx),
        "osc52": execute_osc52_scenario(ctx),
        "interactive": execute_interactive_scenario(ctx),
        "panic": execute_panic_scenario(ctx),
    }


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
    ctx: ScenarioRuntimeContext,
    scenario_overrides: Mapping[str, Callable[[ScenarioRuntimeContext], dict[str, object]]] | None = None,
) -> dict[str, object]:
    """Dispatches `scenario_name` and automatically captures `failure-screenshot.png` on failure."""
    table = dict(DEFAULT_SCENARIO_TABLE)
    if scenario_overrides:
        table.update(scenario_overrides)
    runner = table.get(scenario_name)
    if runner is None:
        raise DriverRuntimeError(f"Unsupported scenario '{scenario_name}'.", "Pass --help for valid scenarios.")
    try:
        return runner(ctx)
    except Exception as exc:
        shot = capture_failure_screenshot_safe(getattr(ctx, "vnc", None), getattr(ctx, "artifact_dir", None))
        if shot is not None and isinstance(exc, DriverError):
            exc.recovery_hint = f"{exc.recovery_hint} (Screenshot saved to {shot})"
        raise
