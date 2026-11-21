"""Milestone 0 matrix sweep (`just iterm2 capability_probe matrix`) for rows 1, 4-7, 9-13, 19-20."""

from __future__ import annotations

from collections.abc import Callable
import dataclasses

from driver.cli_args import DriverError
from driver.interactive_steps import (
    finish_interactive_clean_exit,
    prepare_ready_interactive_session,
    run_fenced_vnc_step,
)
from driver.iterm_window import (
    GuestItermSession,
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
    context: ScenarioRuntimeContext,
    is_natural_scrolling: bool,
) -> dict[str, object]:
    set_guest_natural_scrolling(context.vm, is_natural_scrolling)
    label = "on" if is_natural_scrolling else "off"
    session_id = context.next_session_id(f"row1_wheel_{label}")
    session = launch_iterm_session(
        context.vm,
        SessionLaunchSpec(
            session_id=session_id,
            guest_program_path=context.guest_program_path,
            program_args=[],
        ),
    )
    geometry = prepare_ready_interactive_session(
        context.vm, context.vnc, session, f"sync_wheel_{label}", context.timeout_seconds
    )
    wheel_x, wheel_y = cell_center_to_vnc_pixels(geometry, 10, 5)
    btn4_events = run_fenced_vnc_step(
        context.vm,
        context.vnc,
        session,
        f"wheel_btn4_{label}",
        lambda: context.vnc.run_tokens(build_wheel_tokens(wheel_x, wheel_y, "down")),
        context.timeout_seconds,
    )
    btn5_events = run_fenced_vnc_step(
        context.vm,
        context.vnc,
        session,
        f"wheel_btn5_{label}",
        lambda: context.vnc.run_tokens(build_wheel_tokens(wheel_x, wheel_y, "up")),
        context.timeout_seconds,
    )
    finish_interactive_clean_exit(context.vnc, session, context.timeout_seconds)
    return {
        "naturalScrolling": is_natural_scrolling,
        "vncButton4Events": btn4_events,
        "vncButton5Events": btn5_events,
    }


def measure_row_1_wheel_direction(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 1 wheel event direction with macOS natural scrolling off and on."""
    try:
        off_result = _measure_wheel_under_scroll_setting(context, False)
        on_result = _measure_wheel_under_scroll_setting(context, True)
        return {"naturalScrollingOff": off_result, "naturalScrollingOn": on_result}
    finally:
        set_guest_natural_scrolling(context.vm, context.is_initial_natural_scrolling)


def _measure_single_selection_drag(
    context: ScenarioRuntimeContext,
    session: GuestItermSession,
    start_xy: tuple[int, int],
    end_xy: tuple[int, int],
    label: str,
    modifiers: tuple[str, ...],
    command_key: str,
) -> dict[str, object]:
    context.vm.write_guest_clipboard("EMPTY")
    pty_events = run_fenced_vnc_step(
        context.vm,
        context.vnc,
        session,
        f"row4_{label}",
        lambda: context.vnc.run_tokens(build_drag_tokens(start_xy, end_xy, 1, modifiers)),
        context.timeout_seconds,
    )
    auto_clipboard = context.vm.read_guest_clipboard()
    run_fenced_vnc_step(
        context.vm,
        context.vnc,
        session,
        f"row4_cmdc_{label}",
        lambda: context.vnc.run_tokens(build_chord_tokens("c", (command_key,))),
        context.timeout_seconds,
    )
    cmd_c_clipboard = context.vm.read_guest_clipboard()
    return {
        "modifiers": list(modifiers),
        "ptyMouseEventCount": sum(1 for record in pty_events if record.get("event_type") == "mouse"),
        "ptyEvents": pty_events,
        "autoClipboard": auto_clipboard,
        "afterCmdCClipboard": cmd_c_clipboard,
        "bypassedMouseCapture": not any(record.get("event_type") == "mouse" for record in pty_events),
        "copiedBannerPrefix": "SELECTABLE_ROW" in cmd_c_clipboard or "SELECTABLE_ROW" in auto_clipboard,
    }


def measure_row_4_native_selection(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 4 native text selection vs mouse capture across drag modifiers."""
    modifiers = context.vnc.calibrate_modifiers(context.vm)
    session_id = context.next_session_id("row4_selection")
    session = launch_iterm_session(
        context.vm,
        SessionLaunchSpec(
            session_id=session_id,
            guest_program_path=context.guest_program_path,
            program_args=[],
            pre_banner=SELECTION_BANNER,
            should_copy_selection=True,
        ),
    )
    geometry = prepare_ready_interactive_session(
        context.vm, context.vnc, session, "sync_row4", context.timeout_seconds
    )
    start_xy = cell_center_to_vnc_pixels(geometry, 0, 0)
    end_xy = cell_center_to_vnc_pixels(geometry, 15, 0)
    modifier_cases: list[tuple[str, tuple[str, ...]]] = [
        ("none", ()),
        ("shift", (modifiers.shift_key,)),
        ("option", (modifiers.option_key,)),
        ("command", (modifiers.command_key,)),
        ("option_command", (modifiers.option_key, modifiers.command_key)),
    ]
    results = {
        label: _measure_single_selection_drag(
            context, session, start_xy, end_xy, label, modifier_names, modifiers.command_key
        )
        for label, modifier_names in modifier_cases
    }
    finish_interactive_clean_exit(context.vnc, session, context.timeout_seconds)
    return results


def _measure_option_and_enter_subrun(
    context: ScenarioRuntimeContext,
    option_sends: int,
    should_push_kitty: bool,
) -> dict[str, list[dict[str, object]]]:
    modifiers = context.vnc.calibrate_modifiers(context.vm)
    tag = f"opt{option_sends}_{'kitty' if should_push_kitty else 'plain'}"
    session_id = context.next_session_id(f"rows56_{tag}")
    session = launch_iterm_session(
        context.vm,
        SessionLaunchSpec(
            session_id=session_id,
            guest_program_path=context.guest_program_path,
            program_args=["--push-kitty"] if should_push_kitty else [],
            option_sends=option_sends,
        ),
    )
    prepare_ready_interactive_session(
        context.vm, context.vnc, session, f"sync_{tag}", context.timeout_seconds
    )
    chords = [
        ("option_a", "a", (modifiers.option_key,)),
        ("option_enter", "enter", (modifiers.option_key,)),
        ("shift_enter", "enter", (modifiers.shift_key,)),
        ("ctrl_enter", "enter", (modifiers.ctrl_key,)),
    ]
    outcomes: dict[str, list[dict[str, object]]] = {}
    for label, key_name, key_modifiers in chords:
        chord_tokens = build_chord_tokens(key_name, key_modifiers)
        outcomes[label] = run_fenced_vnc_step(
            context.vm,
            context.vnc,
            session,
            f"{tag}_{label}",
            lambda tokens=chord_tokens: context.vnc.run_tokens(tokens),
            context.timeout_seconds,
        )
    finish_interactive_clean_exit(context.vnc, session, context.timeout_seconds)
    return outcomes


def measure_rows_5_and_6_keys(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 5 (Option+a) and Row 6 (Alt/Shift/Ctrl+Enter) across Option & Kitty modes."""
    return {
        "optionDefault_kittyOff": _measure_option_and_enter_subrun(context, 0, False),
        "optionDefault_kittyPushed": _measure_option_and_enter_subrun(context, 0, True),
        "optionEscPlus_kittyOff": _measure_option_and_enter_subrun(context, 2, False),
        "optionEscPlus_kittyPushed": _measure_option_and_enter_subrun(context, 2, True),
    }


def measure_row_7_osc52_iterm2(context: ScenarioRuntimeContext) -> dict[str, object]:
    """Measures Row 7 OSC 52 clipboard writes in iTerm2 with AllowClipboardAccess off vs on."""
    return {
        "allowClipboardAccessOff": _run_single_osc52_probe(
            context, "plain", "tuikit-row7-osc52-off", None, False
        ),
        "allowClipboardAccessOn": _run_single_osc52_probe(
            context, "plain", "tuikit-row7-osc52-on", None, True
        ),
    }


def _measure_tmux_osc52_with_outer_clipboard_on(
    context: ScenarioRuntimeContext,
    tmux_mode: str,
) -> list[dict[str, object]]:
    return [
        _run_single_osc52_probe(context, form, f"tuikit-{tmux_mode}-allow-on-{form}", tmux_mode, True)
        for form in ("plain", "tmux", "both")
    ]


def _run_default_matrix_sweeps(context: ScenarioRuntimeContext) -> dict[str, object]:
    row_1 = measure_row_1_wheel_direction(context)
    row_4 = measure_row_4_native_selection(context)
    rows_5_6 = measure_rows_5_and_6_keys(context)
    row_7 = measure_row_7_osc52_iterm2(context)
    tmux_osc52_allow_on: dict[str, object] = {}
    for mode in ("stock", "full", "cc-stock", "cc-full"):
        sub_context = dataclasses.replace(context, tmux_mode=mode)
        tmux_osc52_allow_on[mode] = _measure_tmux_osc52_with_outer_clipboard_on(sub_context, mode)
    return {
        "scenario": "matrix",
        "row_1_wheel_direction": row_1,
        "row_4_native_selection": row_4,
        "rows_5_6_option_and_enter": rows_5_6,
        "row_7_osc52_iterm2": row_7,
        "rows_9_to_13_osc52_outer_clipboard_enabled": tmux_osc52_allow_on,
    }


def run_iterm2_matrix(
    context: ScenarioRuntimeContext,
    restore_callback: Callable[[], None],
    probe_runner: Callable[[ScenarioRuntimeContext], dict[str, object]] | None = None,
) -> dict[str, object]:
    """Runs the full Milestone 0 iTerm2 matrix and restores guest defaults in `finally`."""
    active_runner = _run_default_matrix_sweeps if probe_runner is None else probe_runner
    try:
        return active_runner(context)
    except Exception as exc:
        screenshot_path = capture_failure_screenshot_safe(
            getattr(context, "vnc", None), getattr(context, "artifact_dir", None)
        )
        if screenshot_path is not None and isinstance(exc, DriverError):
            exc.recovery_hint = f"{exc.recovery_hint} (Screenshot saved to {screenshot_path})"
        raise
    finally:
        restore_callback()
