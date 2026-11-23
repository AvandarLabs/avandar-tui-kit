"""Guest iTerm2 preference management, session launching (`tmux` / `tmux -CC`), and CGWindow queries."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
import json
from pathlib import Path
import shlex

from driver import DEFAULT_CELL_HEIGHT_PT, DEFAULT_CELL_WIDTH_PT, DEFAULT_COLUMNS, DEFAULT_ROWS
from driver.cli_args import DriverRuntimeError
from driver.guest_scripts import (
    ACTIVATE_SETTINGS_BINARY,
    CG_WINDOWS_SCRIPT,
    ITERM_PREFERENCES_UPDATER_SCRIPT,
    PTY_RUNNER_SCRIPT,
)
from driver.tart_vm import GUEST_SHARED_ROOT, TartVmSession
from driver.vnc_input import VncController, WindowGeometry, normalize_cell_size_pt
from driver.wait_helpers import poll_until, read_complete_jsonl_records


@dataclass(frozen=True)
class GuestDefaultsBackup:
    """Tracks original guest iTerm2 plist, scroll direction, and Sequoia desktop-click state."""

    plist_backup_guest_path: str
    had_swipe_direction: bool
    is_natural_swipe_direction: bool
    had_click_show_desktop: bool = False
    is_click_show_desktop_enabled: bool = True


@dataclass(frozen=True)
class SessionLaunchSpec:
    """Configuration for a single iTerm2 session inside the guest VM."""

    session_id: str
    guest_program_path: str
    program_args: list[str]
    tmux_mode: str | None = None
    guest_tmux_conf_path: str | None = None
    pre_banner: str | None = None
    cols: int = DEFAULT_COLUMNS
    rows: int = DEFAULT_ROWS
    option_sends: int = 0
    should_allow_clipboard_access: bool = False
    should_copy_selection: bool = True


@dataclass(frozen=True)
class GuestItermSession:
    """Paths and handles for an active iTerm2 session sharing logs via VirtioFS."""

    session_id: str
    host_log_path: Path
    host_meta_path: Path
    host_exit_path: Path
    tmux_mode: str | None
    guest_tmux_socket: str | None


def ensure_guest_helper_scripts(vm: TartVmSession) -> None:
    """Writes reusable CoreGraphics and PTY helper scripts into the shared directory."""
    cg_path = vm.shared_dir / "_cg_windows.py"
    if not cg_path.is_file():
        cg_path.write_text(CG_WINDOWS_SCRIPT, encoding="utf-8")
    runner_path = vm.shared_dir / "_pty_runner.py"
    if not runner_path.is_file():
        runner_path.write_text(PTY_RUNNER_SCRIPT, encoding="utf-8")


def backup_guest_defaults(vm: TartVmSession) -> GuestDefaultsBackup:
    """Snapshots iTerm2 plist, scroll direction, and Sequoia wallpaper-click setting."""
    backup_path = "/tmp/tuikit_iterm2_plist_backup.plist"
    vm.exec_guest(f"cp ~/Library/Preferences/com.googlecode.iterm2.plist {backup_path}")
    swipe_process = vm.exec_guest("defaults read -g com.apple.swipescrolldirection", should_check=False)
    had_swipe_direction = swipe_process.returncode == 0
    is_natural_swipe_direction = (
        swipe_process.stdout.strip() in ("1", "true", "TRUE", "YES") if had_swipe_direction else True
    )
    desktop_process = vm.exec_guest(
        "defaults read com.apple.WindowManager EnableStandardClickToShowDesktop",
        should_check=False,
    )
    had_click_show_desktop = desktop_process.returncode == 0
    is_click_show_desktop_enabled = (
        desktop_process.stdout.strip() in ("1", "true", "TRUE", "YES")
        if had_click_show_desktop
        else True
    )
    vm.exec_guest(
        f"defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool false && "
        f"{ACTIVATE_SETTINGS_BINARY} -u 2>/dev/null || true",
        should_check=False,
    )
    return GuestDefaultsBackup(
        backup_path,
        had_swipe_direction,
        is_natural_swipe_direction,
        had_click_show_desktop,
        is_click_show_desktop_enabled,
    )


def restore_guest_defaults(vm: TartVmSession, backup: GuestDefaultsBackup) -> None:
    """Restores guest iTerm2 preferences, scroll setting, and WindowManager state, then deletes backup."""
    swipe_flag = "true" if backup.is_natural_swipe_direction else "false"
    swipe_restore = (
        f"defaults write -g com.apple.swipescrolldirection -bool {swipe_flag}"
        if backup.had_swipe_direction
        else "defaults delete -g com.apple.swipescrolldirection 2>/dev/null || true"
    )
    desktop_flag = "true" if backup.is_click_show_desktop_enabled else "false"
    desktop_restore = (
        f"defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool {desktop_flag}"
        if backup.had_click_show_desktop
        else "defaults delete com.apple.WindowManager EnableStandardClickToShowDesktop 2>/dev/null || true"
    )
    quoted_backup = shlex.quote(backup.plist_backup_guest_path)
    restore_command = "\n".join(
        [
            "killall -9 iTerm2 2>/dev/null || true",
            "killall -9 iTermServer 2>/dev/null || true",
            # Import through CFPreferences so a later daemon flush cannot restore
            # the cached test values over the on-disk backup.
            f"test -f {quoted_backup} && defaults import com.googlecode.iterm2 {quoted_backup} || true",
            f"rm -f {quoted_backup}",
            swipe_restore,
            desktop_restore,
            "killall cfprefsd 2>/dev/null || true",
            f"{ACTIVATE_SETTINGS_BINARY} -u 2>/dev/null || true",
        ]
    )
    vm.exec_guest(restore_command, should_check=False)


def set_guest_natural_scrolling(vm: TartVmSession, is_enabled: bool) -> None:
    """Sets macOS global `com.apple.swipescrolldirection` and activates it immediately."""
    flag = "true" if is_enabled else "false"
    vm.exec_guest(f"defaults write -g com.apple.swipescrolldirection -bool {flag} && {ACTIVATE_SETTINGS_BINARY} -u")


def configure_iterm_preferences(vm: TartVmSession, spec: SessionLaunchSpec) -> None:
    """Terminates stale iTerm2 processes and writes deterministic session preferences."""
    (vm.shared_dir / "_iterm_prefs_updater.py").write_text(
        ITERM_PREFERENCES_UPDATER_SCRIPT, encoding="utf-8"
    )
    preferences_config = {
        "cols": spec.cols,
        "rows": spec.rows,
        "option_sends": spec.option_sends,
        "allow_clipboard": spec.should_allow_clipboard_access,
        "copy_selection": spec.should_copy_selection,
    }
    (vm.shared_dir / "_iterm_prefs_cfg.json").write_text(json.dumps(preferences_config), encoding="utf-8")
    vm.exec_guest(
        "\n".join(
            [
                "killall -9 iTerm2 2>/dev/null || true",
                "killall -9 iTermServer 2>/dev/null || true",
                "rm -rf ~/Library/Saved\\ Application\\ State/com.googlecode.iterm2.savedState",
                "rm -rf ~/Library/Application\\ Support/iTerm2/SavedState",
                f'python3 "{GUEST_SHARED_ROOT}/_iterm_prefs_updater.py"',
                "killall cfprefsd 2>/dev/null || true",
            ]
        )
    )


def _build_launch_command_body(
    spec: SessionLaunchSpec,
    guest_spec_json: str,
    guest_socket: str | None,
) -> str:
    runner_invocation = f'python3 "{GUEST_SHARED_ROOT}/_pty_runner.py" {shlex.quote(guest_spec_json)}'
    if spec.tmux_mode is None:
        return f"#!/bin/bash\nexport LANG=en_US.UTF-8\nexec {runner_invocation}\n"
    assert guest_socket is not None and spec.guest_tmux_conf_path is not None
    cc_flag = "-CC " if spec.tmux_mode.startswith("cc-") else ""
    quoted_runner = shlex.quote(runner_invocation)
    quoted_config_path = shlex.quote(spec.guest_tmux_conf_path)
    return "\n".join(
        [
            "#!/bin/bash",
            "export LANG=en_US.UTF-8",
            f"/opt/homebrew/bin/tmux -S {shlex.quote(guest_socket)} -f {quoted_config_path} {cc_flag}new-session -x {spec.cols} -y {spec.rows} {quoted_runner}",
            f"/opt/homebrew/bin/tmux -S {shlex.quote(guest_socket)} kill-server 2>/dev/null || true",
            "",
        ]
    )


def launch_iterm_session(vm: TartVmSession, spec: SessionLaunchSpec) -> GuestItermSession:
    """Configures iTerm2 preferences and opens a `.command` wrapper for `spec`."""
    ensure_guest_helper_scripts(vm)
    configure_iterm_preferences(vm, spec)
    session_id = spec.session_id
    host_log = vm.shared_dir / f"{session_id}.jsonl"
    host_meta = vm.shared_dir / f"{session_id}.meta.json"
    host_exit = vm.shared_dir / f"{session_id}.exit.json"
    for stale_path in (host_log, Path(f"{host_log}.ready"), host_meta, host_exit):
        stale_path.unlink(missing_ok=True)
    guest_log = f"{GUEST_SHARED_ROOT}/{session_id}.jsonl"
    guest_socket = f"/tmp/tuikit-{session_id}.sock" if spec.tmux_mode is not None else None
    runner_spec = {
        "program_path": spec.guest_program_path,
        "program_args": ["--log", guest_log, *spec.program_args],
        "meta_path": f"{GUEST_SHARED_ROOT}/{session_id}.meta.json",
        "exit_path": f"{GUEST_SHARED_ROOT}/{session_id}.exit.json",
        "pre_banner": spec.pre_banner,
        "tmux_sock": guest_socket,
        "tmux_bin": "/opt/homebrew/bin/tmux",
        "wait_cc_attach": bool(spec.tmux_mode and spec.tmux_mode.startswith("cc-")),
        "query_decrqm": True,
    }
    (vm.shared_dir / f"{session_id}.spec.json").write_text(json.dumps(runner_spec), encoding="utf-8")
    guest_spec_path = f"{GUEST_SHARED_ROOT}/{session_id}.spec.json"
    command_body = _build_launch_command_body(spec, guest_spec_path, guest_socket)
    (vm.shared_dir / f"{session_id}.command").write_text(command_body, encoding="utf-8")
    vm.exec_guest(
        f'cp "{GUEST_SHARED_ROOT}/{session_id}.command" /tmp/tuikit_{session_id}.command && '
        f"chmod +x /tmp/tuikit_{session_id}.command && "
        f"open -a /Applications/iTerm.app /tmp/tuikit_{session_id}.command"
    )
    return GuestItermSession(session_id, host_log, host_meta, host_exit, spec.tmux_mode, guest_socket)


def query_iterm_windows(vm: TartVmSession) -> list[dict[str, object]]:
    """Queries on-screen iTerm2 windows in front-to-back z-order via CoreGraphics."""
    ensure_guest_helper_scripts(vm)
    raw = vm.exec_guest(f'python3 "{GUEST_SHARED_ROOT}/_cg_windows.py"', 10.0).stdout.strip()
    if not raw:
        return []
    parsed = json.loads(raw)
    return parsed if isinstance(parsed, list) else []


def has_popup_menu_window(windows: Sequence[dict[str, object]]) -> bool:
    """Returns True when an iTerm2 context/popup menu window (`kCGWindowLayer > 0`) is open."""
    return any(int(window_info.get("kCGWindowLayer", 0)) > 0 for window_info in windows)  # type: ignore[arg-type]


def _is_terminal_grid_window(window_info: dict[str, object]) -> bool:
    if int(window_info.get("kCGWindowLayer", -1)) != 0:  # type: ignore[arg-type]
        return False
    bounds = window_info.get("kCGWindowBounds")
    if not isinstance(bounds, dict):
        return False
    return (
        float(bounds.get("Width", 0.0)) >= 400.0
        and float(bounds.get("Height", 0.0)) >= 300.0
        and float(bounds.get("X", -1.0)) >= 0.0
        and float(bounds.get("Y", -1.0)) >= 0.0
    )


def select_active_iterm_window(windows: Sequence[dict[str, object]]) -> dict[str, object]:
    """Selects the frontmost normal (`kCGWindowLayer == 0`) iTerm2 terminal window."""
    normal = [window_info for window_info in windows if _is_terminal_grid_window(window_info)]
    if not normal:
        raise DriverRuntimeError(
            "No on-screen iTerm2 terminal window found in CoreGraphics window list.",
            "Inspect failure-screenshot.png to verify iTerm2 opened the session window.",
        )
    return normal[0]


def dismiss_popup_menus_if_any(vm: TartVmSession, vnc: VncController, timeout_seconds: float) -> None:
    """Dismisses any open iTerm2 right-click / Ctrl-click context menu before sending F2."""
    if not has_popup_menu_window(query_iterm_windows(vm)):
        return
    vnc.run_tokens(["key", "esc"])
    poll_until(
        lambda: True if not has_popup_menu_window(query_iterm_windows(vm)) else None,
        timeout_seconds,
        "iTerm2 context menu dismissal",
    )


def query_frontmost_app_name(vm: TartVmSession) -> str:
    """Returns `'iTerm2'` when iTerm2 is frontmost, or the active frontmost app info string."""
    output = vm.exec_guest(
        'lsappinfo info -only bundleid -only name "$(lsappinfo front)"',
        timeout_seconds=10.0,
        should_check=False,
    ).stdout
    if "com.googlecode.iterm2" in output or '"iTerm2"' in output:
        return "iTerm2"
    return output.strip()


def _resolve_live_pane_grid(
    vm: TartVmSession, session: GuestItermSession, records: Sequence[dict[str, object]]
) -> tuple[int, int]:
    columns, rows = DEFAULT_COLUMNS, DEFAULT_ROWS
    if session.host_meta_path.is_file():
        session_metadata = json.loads(session.host_meta_path.read_text(encoding="utf-8"))
        columns = int(session_metadata.get("cols", DEFAULT_COLUMNS))
        rows = int(session_metadata.get("rows", DEFAULT_ROWS))
    for record in records:
        if record.get("kind") == "event" and record.get("event_type") == "resize":
            columns = int(record.get("columns", columns))  # type: ignore[arg-type]
            rows = int(record.get("rows", rows))  # type: ignore[arg-type]
    if session.guest_tmux_socket:
        quoted_socket_path = shlex.quote(session.guest_tmux_socket)
        pane_output = vm.exec_guest(
            f"/opt/homebrew/bin/tmux -S {quoted_socket_path} display-message -p '#{{pane_width}}:#{{pane_height}}'",
            timeout_seconds=5.0,
            should_check=False,
        ).stdout.strip()
        if ":" in pane_output and all(part.isdigit() and int(part) > 0 for part in pane_output.split(":", 1)):
            pane_width, pane_height = pane_output.split(":", 1)
            columns, rows = int(pane_width), int(pane_height)
    return columns, rows


def resolve_session_geometry(
    vm: TartVmSession,
    session: GuestItermSession,
    timeout_seconds: float,
) -> WindowGeometry:
    """Combines live CoreGraphics window bounds with outer PTY grid dimensions and cell size."""
    active_window = poll_until(
        lambda: next(
            (window_info for window_info in query_iterm_windows(vm) if _is_terminal_grid_window(window_info)),
            None,
        ),
        timeout_seconds,
        "on-screen iTerm2 window bounds",
    )
    bounds = active_window["kCGWindowBounds"]  # type: ignore[index]
    records = read_complete_jsonl_records(session.host_log_path)
    columns, inner_rows = _resolve_live_pane_grid(vm, session, records)
    outer_rows = inner_rows + (1 if session.tmux_mode in ("stock", "full") else 0)
    cell_width, cell_height = DEFAULT_CELL_WIDTH_PT, DEFAULT_CELL_HEIGHT_PT
    for record in records:
        if record.get("kind") == "section" and record.get("section") == "cell_size" and record.get("answered"):
            cell_width = float(record.get("width_px", DEFAULT_CELL_WIDTH_PT))  # type: ignore[arg-type]
            cell_height = float(record.get("height_px", DEFAULT_CELL_HEIGHT_PT))  # type: ignore[arg-type]
    window_x, window_y = float(bounds["X"]), float(bounds["Y"])  # type: ignore[index]
    window_width, window_height = float(bounds["Width"]), float(bounds["Height"])  # type: ignore[index]
    cell_width, cell_height = normalize_cell_size_pt(
        cell_width, cell_height, window_width, window_height, columns, outer_rows, 2.0
    )
    return WindowGeometry(
        window_x, window_y, window_width, window_height, columns, outer_rows, cell_width, cell_height, 2.0
    )
