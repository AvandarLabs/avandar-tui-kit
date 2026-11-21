"""Interactive Layer 6 VNC pointer, keyboard, paste, focus, and Kitty Enter step runners."""

from __future__ import annotations

import json
from typing import Callable

from driver.cli_args import DriverRuntimeError
from driver.iterm_window import (
    GuestItermSession, dismiss_popup_menus_if_any, query_frontmost_app_name, resolve_session_geometry,
)
from driver.tart_vm import TartVmSession
from driver.vnc_input import (
    ModifierMap, VncController, WindowGeometry, build_chord_tokens, build_click_tokens,
    build_drag_tokens, build_move_tokens, build_wheel_tokens, cell_center_to_vnc_pixels,
    outside_focus_point_vnc_pixels, titlebar_point_vnc_pixels,
)
from driver.wait_helpers import (
    extract_fence_slice, has_delivered_paste_payload, parse_decrqm_snapshot,
    poll_until, read_all_jsonl_records, read_complete_jsonl_records, wait_for_probe_ready,
)


def ensure_iterm_window_focused(
    vm: TartVmSession,
    vnc: VncController,
    geometry: WindowGeometry,
    timeout_seconds: float,
) -> None:
    """Activates iTerm2 via its title bar until it becomes the frontmost macOS app."""
    tx, ty = titlebar_point_vnc_pixels(geometry)

    def _activate_until_frontmost() -> bool | None:
        if query_frontmost_app_name(vm) == "iTerm2":
            return True
        vnc.run_tokens(build_click_tokens(tx, ty, button=1))
        return True if query_frontmost_app_name(vm) == "iTerm2" else None

    poll_until(_activate_until_frontmost, timeout_seconds, "iTerm2 frontmost activation", 0.15)


def run_fenced_vnc_step(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    step_label: str,
    action: Callable[[], None],
    timeout_seconds: float,
    dismiss_menu_before_fence: bool = False,
) -> list[dict[str, object]]:
    """Executes `action`, optionally dismisses any popup menu, sends F2, and returns the slice."""
    start_count = len(read_complete_jsonl_records(session.host_log_path))
    action()
    if dismiss_menu_before_fence:
        dismiss_popup_menus_if_any(vm, vnc, timeout_seconds)
    vnc.send_fence_key()

    def _poll_slice() -> list[dict[str, object]] | None:
        records = read_complete_jsonl_records(session.host_log_path)
        sliced = extract_fence_slice(records, start_count)
        if sliced is not None:
            return sliced
        if session.host_exit_path.is_file():
            raise DriverRuntimeError(
                f"Session exited unexpectedly while waiting for F2 fence '{step_label}'.",
                "Inspect failure-screenshot.png and session logs.",
            )
        return None

    return poll_until(_poll_slice, timeout_seconds, f"F2 fence for step '{step_label}'")


def _has_mouse_event(
    records: Sequence[dict[str, object]],
    mouse_kind: str,
    col: int | None = None,
    row: int | None = None,
) -> bool:
    for rec in records:
        if rec.get("event_type") != "mouse" or rec.get("mouse_kind") != mouse_kind:
            continue
        if col is not None and rec.get("column") != col:
            continue
        if row is not None and rec.get("row") != row:
            continue
        return True
    return False


def assert_iterm2_pointer_results(
    pointer_steps: dict[str, list[dict[str, object]]],
    tmux_mode: str | None,
) -> None:
    """Validates Rule M1 hover suppression and expected mouse events across modes."""
    hover_moves = [r for r in pointer_steps["hover"] if r.get("mouse_kind") == "moved"]
    if hover_moves:
        raise DriverRuntimeError(
            f"Rule M1 violation: mode 1002 emitted {len(hover_moves)} bare hover move events.",
            "Ensure mode 1003 is never enabled by setup M1.",
        )
    all_mouse = [
        rec
        for step_records in pointer_steps.values()
        for rec in step_records
        if rec.get("event_type") == "mouse"
    ]
    if tmux_mode in ("cc-stock", "cc-full") and not all_mouse:
        return
    checks = [
        (_has_mouse_event(pointer_steps["left_click"], "down:left", 10, 5), "left_click down:left@(10,5)"),
        (_has_mouse_event(pointer_steps["left_click"], "up:left", 10, 5), "left_click up:left@(10,5)"),
        (_has_mouse_event(pointer_steps["left_drag"], "down:left", 12, 6), "left_drag down:left@(12,6)"),
        (_has_mouse_event(pointer_steps["left_drag"], "drag:left"), "left_drag drag:left"),
        (_has_mouse_event(pointer_steps["left_drag"], "up:left", 18, 6), "left_drag up:left@(18,6)"),
        (_has_mouse_event(pointer_steps["wheel_down"], "scroll_down", 10, 5), "wheel_down@(10,5)"),
        (_has_mouse_event(pointer_steps["wheel_up"], "scroll_up", 10, 5), "wheel_up@(10,5)"),
    ]
    missing = [label for ok, label in checks if not ok]
    if missing:
        actual = [(r.get("mouse_kind"), r.get("column"), r.get("row")) for r in pointer_steps["left_click"]]
        raise DriverRuntimeError(
            f"Missing expected pointer events in mode '{tmux_mode or 'direct'}': {', '.join(missing)} (left_click={actual}).",
            "Inspect failure-screenshot.png to verify window alignment and cell coordinates.",
        )


def run_pointer_steps(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    geom: WindowGeometry,
    mods: ModifierMap,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Runs basic and modified pointer steps over VNC with F2 fences."""
    c = lambda col, row: cell_center_to_vnc_pixels(geom, col, row)  # noqa: E731
    run = lambda name, fn, menu=False: run_fenced_vnc_step(  # noqa: E731
        vm, vnc, session, name, fn, timeout_seconds, dismiss_menu_before_fence=menu
    )
    steps = {
        "hover": run("hover", lambda: vnc.run_tokens(build_move_tokens([c(2, 2), c(4, 2), c(6, 2)]))),
        "left_click": run("left_click", lambda: vnc.run_tokens(build_click_tokens(*c(10, 5), 1))),
        "left_drag": run("left_drag", lambda: vnc.run_tokens(build_drag_tokens(c(12, 6), c(18, 6), 1))),
        "wheel_down": run("wheel_down", lambda: vnc.run_tokens(build_wheel_tokens(*c(10, 5), "down"))),
        "wheel_up": run("wheel_up", lambda: vnc.run_tokens(build_wheel_tokens(*c(10, 5), "up"))),
        "right_click": run("right_click", lambda: vnc.run_tokens(build_click_tokens(*c(15, 7), 3)), True),
        "shift_click": run(
            "shift_click", lambda: vnc.run_tokens(build_click_tokens(*c(11, 5), 1, (mods.shift_key,)))
        ),
        "alt_click": run(
            "alt_click", lambda: vnc.run_tokens(build_click_tokens(*c(12, 5), 1, (mods.option_key,)))
        ),
        "cmd_click": run(
            "cmd_click", lambda: vnc.run_tokens(build_click_tokens(*c(12, 5), 1, (mods.command_key,)))
        ),
        "alt_drag": run(
            "alt_drag",
            lambda: vnc.run_tokens(build_drag_tokens(c(12, 8), c(16, 8), 1, (mods.option_key,))),
        ),
        "ctrl_click": run(
            "ctrl_click",
            lambda: vnc.run_tokens(build_click_tokens(*c(13, 5), 1, (mods.ctrl_key,))),
            True,
        ),
        "double_click": run(
            "double_click", lambda: vnc.run_tokens(build_click_tokens(*c(14, 5), 1, (), 2))
        ),
    }
    assert_iterm2_pointer_results(steps, session.tmux_mode)
    return steps


def run_keyboard_steps(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    mods: ModifierMap,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Runs standard navigation, function, Control, Option, and Enter chord probes."""
    chord_specs: list[tuple[str, str, tuple[str, ...]]] = [
        ("arrow_up", "up", ()),
        ("f1", "f1", ()),
        ("ctrl_a", "a", (mods.ctrl_key,)),
        ("alt_a", "a", (mods.option_key,)),
        ("alt_enter", "enter", (mods.option_key,)),
        ("shift_enter", "enter", (mods.shift_key,)),
        ("ctrl_enter", "enter", (mods.ctrl_key,)),
    ]
    results: dict[str, list[dict[str, object]]] = {}
    for label, key_name, key_mods in chord_specs:
        results[label] = run_fenced_vnc_step(
            vm,
            vnc,
            session,
            label,
            lambda kn=key_name, km=key_mods: vnc.run_tokens(build_chord_tokens(kn, km)),
            timeout_seconds,
        )
    if not any(r.get("code") == "up" for r in results["arrow_up"]):
        raise DriverRuntimeError("Missing ArrowUp key event.", "Check VNC keyboard focus.")
    if not any(r.get("code") == "f1" for r in results["f1"]):
        raise DriverRuntimeError("Missing F1 key event.", "Check VNC function-key delivery.")
    if not any(r.get("code") == "char:a" and r.get("ctrl") is True for r in results["ctrl_a"]):
        raise DriverRuntimeError("Missing Ctrl+a key event.", "Check VNC Control modifier delivery.")
    return results


def run_paste_step(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    mods: ModifierMap,
    timeout_seconds: float,
) -> list[dict[str, object]]:
    """Loads `alpha\\nbeta` into guest pasteboard, presses Cmd+V over VNC, and verifies arrival."""
    start_count = len(read_complete_jsonl_records(session.host_log_path))
    vm.write_guest_clipboard("alpha\nbeta")
    vnc.run_tokens(build_chord_tokens("v", (mods.command_key,)))
    poll_until(
        lambda: (
            True
            if has_delivered_paste_payload(read_complete_jsonl_records(session.host_log_path)[start_count:])
            else None
        ),
        timeout_seconds,
        "Cmd+V paste arrival before F2 fence",
    )
    vnc.send_fence_key()
    paste_events = poll_until(
        lambda: extract_fence_slice(read_complete_jsonl_records(session.host_log_path), start_count),
        timeout_seconds,
        "F2 fence after Cmd+V paste",
    )
    has_bracketed = any(
        rec.get("event_type") == "paste" and rec.get("text") in ("alpha\rbeta", "alpha\nbeta")
        for rec in paste_events
    )
    if not has_bracketed:
        raise DriverRuntimeError(
            f"Expected bracketed paste 'alpha\\rbeta' or 'alpha\\nbeta', got {paste_events}.",
            "Verify bracketed paste mode (?2004h) and Cmd+V modifier calibration.",
        )
    return paste_events


def _blur_iterm_to_desktop(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    geom: WindowGeometry,
    expect_focus: bool,
    timeout_seconds: float,
) -> int:
    blur_start = len(read_complete_jsonl_records(session.host_log_path))
    out_x, out_y = outside_focus_point_vnc_pixels(geom)
    vnc.run_tokens(build_click_tokens(out_x, out_y, button=1))
    poll_until(
        lambda: True if query_frontmost_app_name(vm) != "iTerm2" else None,
        timeout_seconds,
        "Finder activation after desktop click",
    )
    if expect_focus:
        poll_until(
            lambda: (
                True
                if any(
                    r.get("event_type") == "focus" and r.get("focused") is False
                    for r in read_complete_jsonl_records(session.host_log_path)[blur_start:]
                )
                else None
            ),
            timeout_seconds,
            "FocusLost event in probe log",
        )
    return blur_start


def run_focus_steps(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    geom: WindowGeometry,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Blurs iTerm2 by clicking Finder desktop, then restores focus via the iTerm2 title bar."""
    expect_focus = session.tmux_mode in (None, "full")
    blur_start = _blur_iterm_to_desktop(vm, vnc, session, geom, expect_focus, timeout_seconds)
    ensure_iterm_window_focused(vm, vnc, geom, timeout_seconds)
    vnc.send_fence_key()
    all_events = poll_until(
        lambda: extract_fence_slice(read_complete_jsonl_records(session.host_log_path), blur_start),
        timeout_seconds,
        "F2 fence after focus regain",
    )
    focus_lost = [r for r in all_events if r.get("event_type") == "focus" and r.get("focused") is False]
    focus_gained = [r for r in all_events if r.get("event_type") == "focus" and r.get("focused") is True]
    if expect_focus and (not focus_lost or not focus_gained):
        raise DriverRuntimeError("Missing FocusLost or FocusGained event on window refocus.", "Check mode ?1004h.")
    return {"focusLostEvents": focus_lost, "focusGainedEvents": focus_gained}


def wait_for_session_exit_and_teardown(
    session: GuestItermSession,
    timeout_seconds: float,
) -> dict[str, object]:
    """Waits for `<session>.exit.json`, parses DECRQM teardown state, and validates clean reset."""
    exit_data = poll_until(
        lambda: (
            json.loads(session.host_exit_path.read_text(encoding="utf-8"))
            if session.host_exit_path.is_file()
            else None
        ),
        timeout_seconds,
        f"session '{session.session_id}' exit status",
    )
    teardown = parse_decrqm_snapshot(str(exit_data.get("decrqm", "")))
    if exit_data.get("decrqm"):
        if teardown["mouseTrackingMode"] != "none" or teardown["bracketedPasteMode"] or teardown["sendFocusMode"]:
            raise DriverRuntimeError(
                f"Terminal modes leaked after session exit: {teardown}.",
                "Verify TerminalSessionGuard Drop/teardown resets mouse, paste, and focus modes.",
            )
    return {
        "exitCode": int(exit_data.get("exitCode", -1)),
        "signal": int(exit_data.get("signal", 0)),
        "tmuxBuffer": exit_data.get("tmuxBuffer"),
        "terminalStateAfterExit": teardown,
    }


def finish_interactive_clean_exit(
    vnc: VncController,
    session: GuestItermSession,
    timeout_seconds: float,
) -> dict[str, object]:
    """Sends Ctrl+C over VNC and verifies clean exit code 0, summary record, and mode teardown."""
    vnc.run_tokens(["key", "ctrl-c"])
    exit_outcome = wait_for_session_exit_and_teardown(session, timeout_seconds)
    if exit_outcome["exitCode"] != 0:
        raise DriverRuntimeError(
            f"Expected exit code 0 on Ctrl+C clean exit, got {exit_outcome['exitCode']}.",
            "Verify Ctrl+C exits interactive capability_probe cleanly.",
        )
    records = read_all_jsonl_records(session.host_log_path)
    has_summary = any(r.get("kind") == "summary" and r.get("mode") == "interactive" for r in records)
    has_ctrl_c = any(
        r.get("kind") == "event" and r.get("code") == "char:c" and r.get("ctrl") is True for r in records
    )
    if not (has_summary and has_ctrl_c):
        raise DriverRuntimeError("Missing interactive summary or Ctrl+C event record.", "Inspect JSONL log.")
    return exit_outcome


def prepare_ready_interactive_session(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    fence_label: str,
    timeout_seconds: float,
) -> WindowGeometry:
    """Waits for `<log>.ready`, resolves window geometry, activates iTerm2, and sends sync fence."""
    wait_for_probe_ready(session.host_log_path, timeout_seconds, session.host_exit_path)
    geom = resolve_session_geometry(vm, session, timeout_seconds)
    ensure_iterm_window_focused(vm, vnc, geom, timeout_seconds)
    run_fenced_vnc_step(vm, vnc, session, fence_label, lambda: None, timeout_seconds)
    if session.tmux_mode and session.tmux_mode.startswith("cc-"):
        geom = resolve_session_geometry(vm, session, timeout_seconds)
        ensure_iterm_window_focused(vm, vnc, geom, timeout_seconds)
    return geom
