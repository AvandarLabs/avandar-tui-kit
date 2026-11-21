"""Interactive Layer 6 VNC pointer, keyboard, paste, focus, and Kitty Enter step runners."""

from __future__ import annotations

from collections.abc import Callable, Sequence
import json

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
    vm: TartVmSession, vnc: VncController, geometry: WindowGeometry, timeout_seconds: float,
) -> None:
    """Activates iTerm2 via its title bar until it becomes the frontmost macOS app."""
    title_x, title_y = titlebar_point_vnc_pixels(geometry)

    def _activate_until_frontmost() -> bool | None:
        if query_frontmost_app_name(vm) == "iTerm2":
            return True
        vnc.run_tokens(build_click_tokens(title_x, title_y, button=1))
        return True if query_frontmost_app_name(vm) == "iTerm2" else None

    poll_until(_activate_until_frontmost, timeout_seconds, "iTerm2 frontmost activation", 0.15)


def run_fenced_vnc_step(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    step_label: str,
    action: Callable[[], None],
    timeout_seconds: float,
    should_dismiss_menu_before_fence: bool = False,
) -> list[dict[str, object]]:
    """Executes `action`, optionally dismisses any popup menu, sends F2, and returns the slice."""
    start_count = len(read_complete_jsonl_records(session.host_log_path))
    action()
    if should_dismiss_menu_before_fence:
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
    for record in records:
        if record.get("event_type") != "mouse" or record.get("mouse_kind") != mouse_kind:
            continue
        if col is not None and record.get("column") != col:
            continue
        if row is not None and record.get("row") != row:
            continue
        return True
    return False


def assert_iterm2_pointer_results(
    pointer_steps: dict[str, list[dict[str, object]]], tmux_mode: str | None,
) -> None:
    """Validates Rule M1 hover suppression and expected mouse events across modes."""
    hover_moves = [record for record in pointer_steps["hover"] if record.get("mouse_kind") == "moved"]
    if hover_moves:
        raise DriverRuntimeError(
            f"Rule M1 violation: mode 1002 emitted {len(hover_moves)} bare hover move events.",
            "Ensure mode 1003 is never enabled by setup M1.",
        )
    checks = [
        (_has_mouse_event(pointer_steps["left_click"], "down:left", 10, 5), "left_click down:left@(10,5)"),
        (_has_mouse_event(pointer_steps["left_click"], "up:left", 10, 5), "left_click up:left@(10,5)"),
        (_has_mouse_event(pointer_steps["left_drag"], "down:left", 12, 6), "left_drag down:left@(12,6)"),
        (_has_mouse_event(pointer_steps["left_drag"], "drag:left"), "left_drag drag:left"),
        (_has_mouse_event(pointer_steps["left_drag"], "up:left", 18, 6), "left_drag up:left@(18,6)"),
        (_has_mouse_event(pointer_steps["wheel_down"], "scroll_down", 10, 5), "wheel_down@(10,5)"),
        (_has_mouse_event(pointer_steps["wheel_up"], "scroll_up", 10, 5), "wheel_up@(10,5)"),
    ]
    missing = [label for is_present, label in checks if not is_present]
    if missing:
        actual = [
            (record.get("mouse_kind"), record.get("column"), record.get("row"))
            for record in pointer_steps["left_click"]
        ]
        raise DriverRuntimeError(
            f"Missing expected pointer events in mode '{tmux_mode or 'direct'}': {', '.join(missing)} (left_click={actual}).",
            "Inspect failure-screenshot.png to verify window alignment and cell coordinates.",
        )


def _build_pointer_step_specs(
    geometry: WindowGeometry, modifiers: ModifierMap,
) -> list[tuple[str, list[str], bool]]:
    def cell_pixels(col: int, row: int) -> tuple[int, int]:
        return cell_center_to_vnc_pixels(geometry, col, row)

    return [
        ("hover", build_move_tokens([cell_pixels(2, 2), cell_pixels(4, 2), cell_pixels(6, 2)]), False),
        ("left_click", build_click_tokens(*cell_pixels(10, 5), 1), False),
        ("left_drag", build_drag_tokens(cell_pixels(12, 6), cell_pixels(18, 6), 1), False),
        ("wheel_down", build_wheel_tokens(*cell_pixels(10, 5), "down"), False),
        ("wheel_up", build_wheel_tokens(*cell_pixels(10, 5), "up"), False),
        ("right_click", build_click_tokens(*cell_pixels(15, 7), 3), True),
        ("shift_click", build_click_tokens(*cell_pixels(11, 5), 1, (modifiers.shift_key,)), False),
        ("alt_click", build_click_tokens(*cell_pixels(12, 5), 1, (modifiers.option_key,)), False),
        ("cmd_click", build_click_tokens(*cell_pixels(12, 5), 1, (modifiers.command_key,)), False),
        ("alt_drag", build_drag_tokens(cell_pixels(12, 8), cell_pixels(16, 8), 1, (modifiers.option_key,)), False),
        ("ctrl_click", build_click_tokens(*cell_pixels(13, 5), 1, (modifiers.ctrl_key,)), True),
        ("double_click", build_click_tokens(*cell_pixels(14, 5), 1, (), 2), False),
    ]


def run_pointer_steps(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    geometry: WindowGeometry,
    modifiers: ModifierMap,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Runs basic and modified pointer steps over VNC with F2 fences."""
    steps: dict[str, list[dict[str, object]]] = {}
    for step_name, step_tokens, should_dismiss_menu in _build_pointer_step_specs(geometry, modifiers):
        steps[step_name] = run_fenced_vnc_step(
            vm, vnc, session, step_name,
            lambda tokens=step_tokens: vnc.run_tokens(tokens),
            timeout_seconds,
            should_dismiss_menu_before_fence=should_dismiss_menu,
        )
    assert_iterm2_pointer_results(steps, session.tmux_mode)
    return steps


def run_keyboard_steps(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    modifiers: ModifierMap,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Runs standard navigation, function, Control, Option, and Enter chord probes."""
    chord_specs: list[tuple[str, str, tuple[str, ...]]] = [
        ("arrow_up", "up", ()),
        ("f1", "f1", ()),
        ("ctrl_a", "a", (modifiers.ctrl_key,)),
        ("alt_a", "a", (modifiers.option_key,)),
        ("alt_enter", "enter", (modifiers.option_key,)),
        ("shift_enter", "enter", (modifiers.shift_key,)),
        ("ctrl_enter", "enter", (modifiers.ctrl_key,)),
    ]
    results: dict[str, list[dict[str, object]]] = {}
    for label, key_name, key_modifiers in chord_specs:
        chord_tokens = build_chord_tokens(key_name, key_modifiers)
        results[label] = run_fenced_vnc_step(
            vm, vnc, session, label, lambda tokens=chord_tokens: vnc.run_tokens(tokens), timeout_seconds,
        )
    if not any(record.get("code") == "up" for record in results["arrow_up"]):
        raise DriverRuntimeError("Missing ArrowUp key event.", "Check VNC keyboard focus.")
    if not any(record.get("code") == "f1" for record in results["f1"]):
        raise DriverRuntimeError("Missing F1 key event.", "Check VNC function-key delivery.")
    if not any(record.get("code") == "char:a" and record.get("ctrl") is True for record in results["ctrl_a"]):
        raise DriverRuntimeError("Missing Ctrl+a key event.", "Check VNC Control modifier delivery.")
    return results


def run_paste_step(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    modifiers: ModifierMap,
    timeout_seconds: float,
) -> list[dict[str, object]]:
    """Loads `alpha\\nbeta` into guest pasteboard, presses Cmd+V over VNC, and verifies arrival."""
    start_count = len(read_complete_jsonl_records(session.host_log_path))
    vm.write_guest_clipboard("alpha\nbeta")
    vnc.run_tokens(build_chord_tokens("v", (modifiers.command_key,)))
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
    has_bracketed_paste = any(
        record.get("event_type") == "paste" and record.get("text") in ("alpha\rbeta", "alpha\nbeta")
        for record in paste_events
    )
    if not has_bracketed_paste:
        raise DriverRuntimeError(
            f"Expected bracketed paste 'alpha\\rbeta' or 'alpha\\nbeta', got {paste_events}.",
            "Verify bracketed paste mode (?2004h) and Cmd+V modifier calibration.",
        )
    return paste_events


def _blur_iterm_to_desktop(
    vm: TartVmSession,
    vnc: VncController,
    session: GuestItermSession,
    geometry: WindowGeometry,
    should_expect_focus: bool,
    timeout_seconds: float,
) -> int:
    blur_start = len(read_complete_jsonl_records(session.host_log_path))
    outside_x, outside_y = outside_focus_point_vnc_pixels(geometry)
    vnc.run_tokens(build_click_tokens(outside_x, outside_y, button=1))
    poll_until(
        lambda: True if query_frontmost_app_name(vm) != "iTerm2" else None,
        timeout_seconds,
        "Finder activation after desktop click",
    )
    if should_expect_focus:
        poll_until(
            lambda: (
                True
                if any(
                    record.get("event_type") == "focus" and record.get("focused") is False
                    for record in read_complete_jsonl_records(session.host_log_path)[blur_start:]
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
    geometry: WindowGeometry,
    timeout_seconds: float,
) -> dict[str, list[dict[str, object]]]:
    """Blurs iTerm2 by clicking Finder desktop, then restores focus via the iTerm2 title bar."""
    should_expect_focus = session.tmux_mode in (None, "full", "cc-stock", "cc-full")
    blur_start = _blur_iterm_to_desktop(vm, vnc, session, geometry, should_expect_focus, timeout_seconds)
    ensure_iterm_window_focused(vm, vnc, geometry, timeout_seconds)
    vnc.send_fence_key()
    all_events = poll_until(
        lambda: extract_fence_slice(read_complete_jsonl_records(session.host_log_path), blur_start),
        timeout_seconds,
        "F2 fence after focus regain",
    )
    focus_lost = [record for record in all_events if record.get("event_type") == "focus" and record.get("focused") is False]
    focus_gained = [record for record in all_events if record.get("event_type") == "focus" and record.get("focused") is True]
    if should_expect_focus and (not focus_lost or not focus_gained):
        raise DriverRuntimeError("Missing FocusLost or FocusGained event on window refocus.", "Check mode ?1004h.")
    return {"focusLostEvents": focus_lost, "focusGainedEvents": focus_gained}


def wait_for_session_exit_and_teardown(
    session: GuestItermSession, timeout_seconds: float,
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
        has_leaked_mode = (
            teardown.get("mouseTrackingMode") != "none"
            or bool(teardown.get("sgrMouseMode"))
            or bool(teardown.get("bracketedPasteMode"))
            or bool(teardown.get("sendFocusMode"))
        )
        if has_leaked_mode:
            raise DriverRuntimeError(
                f"Terminal modes leaked after session exit: {teardown}.",
                "Verify TerminalSessionGuard Drop/teardown resets mouse, SGR mouse, paste, and focus modes.",
            )
    return {
        "exitCode": int(exit_data.get("exitCode", -1)),
        "signal": int(exit_data.get("signal", 0)),
        "tmuxBuffer": exit_data.get("tmuxBuffer"),
        "terminalStateAfterExit": teardown,
    }


def finish_interactive_clean_exit(
    vnc: VncController, session: GuestItermSession, timeout_seconds: float,
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
    has_summary = any(record.get("kind") == "summary" and record.get("mode") == "interactive" for record in records)
    has_ctrl_c = any(
        record.get("kind") == "event" and record.get("code") == "char:c" and record.get("ctrl") is True
        for record in records
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
    geometry = resolve_session_geometry(vm, session, timeout_seconds)
    ensure_iterm_window_focused(vm, vnc, geometry, timeout_seconds)
    run_fenced_vnc_step(vm, vnc, session, fence_label, lambda: None, timeout_seconds)
    if session.tmux_mode and session.tmux_mode.startswith("cc-"):
        geometry = resolve_session_geometry(vm, session, timeout_seconds)
        ensure_iterm_window_focused(vm, vnc, geometry, timeout_seconds)
    return geometry
