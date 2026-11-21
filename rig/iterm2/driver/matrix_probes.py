"""Milestone 0 matrix sweep (`just iterm2 capability_probe matrix`) for rows 1, 4-7, 9-13, 19-20."""

from __future__ import annotations

import dataclasses
from typing import Callable

from driver.cli_args import DriverError
from driver.interactive_steps import (
    finish_interactive_clean_exit,
    prepare_ready_interactive_session,
    run_fenced_vnc_step,
)
from driver.iterm_window import (
    SessionLaunchSpec,
    launch_iterm_session,
    set_guest_natural_scrolling,
)
from driver.scenarios import (
    ScenarioRuntimeContext,
    _run_single_osc52_probe,
    capture_failure_screenshot_safe,
)
from driver.vnc_input import (
    build_chord_tokens,
    build_drag_tokens,
    build_wheel_tokens,
    cell_center_to_vnc_pixels,
)

SELECTION_BANNER = "\x1b[2J\x1b[HSELECTABLE_ROW_ZERO_TEXT_1234567890\r\n"


def _measure_wheel_under_scroll_setting(
    ctx: ScenarioRuntimeContext,
    natural_scrolling: bool,
) -> dict[str, object]:
    set_guest_natural_scrolling(ctx.vm, natural_scrolling)
    label = "on" if natural_scrolling else "off"
    sid = ctx.next_session_id(f"row1_wheel_{label}")
    session = launch_iterm_session(
        ctx.vm,
        SessionLaunchSpec(session_id=sid, guest_program_path=ctx.guest_program_path, program_args=[]),
    )
    geom = prepare_ready_interactive_session(ctx.vm, ctx.vnc, session, f"sync_wheel_{label}", ctx.timeout_seconds)
    wx, wy = cell_center_to_vnc_pixels(geom, 10, 5)
    btn4_events = run_fenced_vnc_step(
        ctx.vm,
        ctx.vnc,
        session,
        f"wheel_btn4_{label}",
        lambda: ctx.vnc.run_tokens(build_wheel_tokens(wx, wy, "down")),
        ctx.timeout_seconds,
    )
    btn5_events = run_fenced_vnc_step(
        ctx.vm,
        ctx.vnc,
        session,
        f"wheel_btn5_{label}",
        lambda: ctx.vnc.run_tokens(build_wheel_tokens(wx, wy, "up")),
        ctx.timeout_seconds,
    )
    finish_interactive_clean_exit(ctx.vnc, session, ctx.timeout_seconds)
    return {
        "naturalScrolling": natural_scrolling,
        "vncButton4Events": btn4_events,
        "vncButton5Events": btn5_events,
    }


def measure_row_1_wheel_direction(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 1 wheel event direction with macOS natural scrolling off and on."""
    try:
        off_result = _measure_wheel_under_scroll_setting(ctx, False)
        on_result = _measure_wheel_under_scroll_setting(ctx, True)
        return {"naturalScrollingOff": off_result, "naturalScrollingOn": on_result}
    finally:
        set_guest_natural_scrolling(ctx.vm, True)


def _measure_single_selection_drag(
    ctx: ScenarioRuntimeContext,
    session: object,
    start_xy: tuple[int, int],
    end_xy: tuple[int, int],
    label: str,
    mod_tuple: tuple[str, ...],
    command_key: str,
) -> dict[str, object]:
    ctx.vm.write_guest_clipboard("EMPTY")
    pty_events = run_fenced_vnc_step(
        ctx.vm,
        ctx.vnc,
        session,  # type: ignore[arg-type]
        f"row4_{label}",
        lambda: ctx.vnc.run_tokens(build_drag_tokens(start_xy, end_xy, 1, mod_tuple)),
        ctx.timeout_seconds,
    )
    auto_clip = ctx.vm.read_guest_clipboard()
    run_fenced_vnc_step(
        ctx.vm,
        ctx.vnc,
        session,  # type: ignore[arg-type]
        f"row4_cmdc_{label}",
        lambda: ctx.vnc.run_tokens(build_chord_tokens("c", (command_key,))),
        ctx.timeout_seconds,
    )
    cmd_c_clip = ctx.vm.read_guest_clipboard()
    return {
        "modifiers": list(mod_tuple),
        "ptyMouseEventCount": sum(1 for r in pty_events if r.get("event_type") == "mouse"),
        "ptyEvents": pty_events,
        "autoClipboard": auto_clip,
        "afterCmdCClipboard": cmd_c_clip,
        "bypassedMouseCapture": not any(r.get("event_type") == "mouse" for r in pty_events),
        "copiedBannerPrefix": "SELECTABLE_ROW" in cmd_c_clip or "SELECTABLE_ROW" in auto_clip,
    }


def measure_row_4_native_selection(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 4 native text selection vs mouse capture across drag modifiers."""
    mods = ctx.vnc.calibrate_modifiers(ctx.vm)
    sid = ctx.next_session_id("row4_selection")
    session = launch_iterm_session(
        ctx.vm,
        SessionLaunchSpec(
            session_id=sid,
            guest_program_path=ctx.guest_program_path,
            program_args=[],
            pre_banner=SELECTION_BANNER,
            copy_selection=True,
        ),
    )
    geom = prepare_ready_interactive_session(ctx.vm, ctx.vnc, session, "sync_row4", ctx.timeout_seconds)
    start_xy = cell_center_to_vnc_pixels(geom, 0, 0)
    end_xy = cell_center_to_vnc_pixels(geom, 15, 0)
    modifier_cases: list[tuple[str, tuple[str, ...]]] = [
        ("none", ()),
        ("shift", (mods.shift_key,)),
        ("option", (mods.option_key,)),
        ("command", (mods.command_key,)),
        ("option_command", (mods.option_key, mods.command_key)),
    ]
    results = {
        label: _measure_single_selection_drag(
            ctx, session, start_xy, end_xy, label, mod_tuple, mods.command_key
        )
        for label, mod_tuple in modifier_cases
    }
    finish_interactive_clean_exit(ctx.vnc, session, ctx.timeout_seconds)
    return results


def _measure_option_and_enter_subrun(
    ctx: ScenarioRuntimeContext,
    option_sends: int,
    push_kitty: bool,
) -> dict[str, list[dict[str, object]]]:
    mods = ctx.vnc.calibrate_modifiers(ctx.vm)
    tag = f"opt{option_sends}_{'kitty' if push_kitty else 'plain'}"
    sid = ctx.next_session_id(f"rows56_{tag}")
    session = launch_iterm_session(
        ctx.vm,
        SessionLaunchSpec(
            session_id=sid,
            guest_program_path=ctx.guest_program_path,
            program_args=["--push-kitty"] if push_kitty else [],
            option_sends=option_sends,
        ),
    )
    prepare_ready_interactive_session(ctx.vm, ctx.vnc, session, f"sync_{tag}", ctx.timeout_seconds)
    chords = [
        ("option_a", "a", (mods.option_key,)),
        ("option_enter", "enter", (mods.option_key,)),
        ("shift_enter", "enter", (mods.shift_key,)),
        ("ctrl_enter", "enter", (mods.ctrl_key,)),
    ]
    out: dict[str, list[dict[str, object]]] = {}
    for label, key_name, key_mods in chords:
        out[label] = run_fenced_vnc_step(
            ctx.vm,
            ctx.vnc,
            session,
            f"{tag}_{label}",
            lambda kn=key_name, km=key_mods: ctx.vnc.run_tokens(build_chord_tokens(kn, km)),
            ctx.timeout_seconds,
        )
    finish_interactive_clean_exit(ctx.vnc, session, ctx.timeout_seconds)
    return out


def measure_rows_5_and_6_keys(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 5 (Option+a) and Row 6 (Alt/Shift/Ctrl+Enter) across Option & Kitty modes."""
    return {
        "optionDefault_kittyOff": _measure_option_and_enter_subrun(ctx, 0, False),
        "optionDefault_kittyPushed": _measure_option_and_enter_subrun(ctx, 0, True),
        "optionEscPlus_kittyOff": _measure_option_and_enter_subrun(ctx, 2, False),
        "optionEscPlus_kittyPushed": _measure_option_and_enter_subrun(ctx, 2, True),
    }


def measure_row_7_osc52_iterm2(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 7 OSC 52 clipboard writes in iTerm2 with AllowClipboardAccess off vs on."""
    return {
        "allowClipboardAccessOff": _run_single_osc52_probe(
            ctx, "plain", "tuikit-row7-osc52-off", None, False
        ),
        "allowClipboardAccessOn": _run_single_osc52_probe(
            ctx, "plain", "tuikit-row7-osc52-on", None, True
        ),
    }


def _measure_tmux_osc52_with_outer_clipboard_on(
    ctx: ScenarioRuntimeContext,
    tmux_mode: str,
) -> list[dict[str, object]]:
    return [
        _run_single_osc52_probe(ctx, form, f"tuikit-{tmux_mode}-allow-on-{form}", tmux_mode, True)
        for form in ("plain", "tmux", "both")
    ]


def _run_default_matrix_sweeps(ctx: ScenarioRuntimeContext) -> dict[str, object]:
    row_1 = measure_row_1_wheel_direction(ctx)
    row_4 = measure_row_4_native_selection(ctx)
    rows_5_6 = measure_rows_5_and_6_keys(ctx)
    row_7 = measure_row_7_osc52_iterm2(ctx)
    tmux_osc52_allow_on: dict[str, object] = {}
    for mode in ("stock", "full", "cc-stock", "cc-full"):
        sub_ctx = dataclasses.replace(ctx, tmux_mode=mode)
        tmux_osc52_allow_on[mode] = _measure_tmux_osc52_with_outer_clipboard_on(sub_ctx, mode)
    return {
        "scenario": "matrix",
        "row_1_wheel_direction": row_1,
        "row_4_native_selection": row_4,
        "rows_5_6_option_and_enter": rows_5_6,
        "row_7_osc52_iterm2": row_7,
        "rows_9_to_13_osc52_outer_clipboard_enabled": tmux_osc52_allow_on,
    }


def run_iterm2_matrix(
    ctx: ScenarioRuntimeContext,
    restore_callback: Callable[[], None],
    probe_runner: Callable[[ScenarioRuntimeContext], dict[str, object]] | None = None,
) -> dict[str, object]:
    """Runs the full Milestone 0 iTerm2 matrix and restores guest defaults in `finally`."""
    active_runner = _run_default_matrix_sweeps if probe_runner is None else probe_runner
    try:
        return active_runner(ctx)
    except Exception as exc:
        shot = capture_failure_screenshot_safe(getattr(ctx, "vnc", None), getattr(ctx, "artifact_dir", None))
        if shot is not None and isinstance(exc, DriverError):
            exc.recovery_hint = f"{exc.recovery_hint} (Screenshot saved to {shot})"
        raise
    finally:
        restore_callback()
