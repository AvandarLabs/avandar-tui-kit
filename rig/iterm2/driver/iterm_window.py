"""Guest iTerm2 preference management, session launching (`tmux` / `tmux -CC`), and CGWindow queries."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import shlex
from typing import Sequence

from driver import DEFAULT_CELL_HEIGHT_PT, DEFAULT_CELL_WIDTH_PT, DEFAULT_COLS, DEFAULT_ROWS
from driver.cli_args import DriverRuntimeError
from driver.tart_vm import GUEST_SHARED_ROOT, TartVmSession
from driver.vnc_input import VncController, WindowGeometry, normalize_cell_size_pt
from driver.wait_helpers import poll_until, read_complete_jsonl_records

ACTIVATE_SETTINGS_BIN = "/System/Library/PrivateFrameworks/SystemAdministration.framework/Resources/activateSettings"

CG_WINDOWS_SCRIPT = """import ctypes, json, plistlib
cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
cg.CGWindowListCopyWindowInfo.argtypes = [ctypes.c_uint32, ctypes.c_uint32]
cg.CGWindowListCopyWindowInfo.restype = ctypes.c_void_p
cf.CFPropertyListCreateData.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_long, ctypes.c_ulong, ctypes.c_void_p]
cf.CFPropertyListCreateData.restype = ctypes.c_void_p
cf.CFDataGetLength.argtypes = [ctypes.c_void_p]
cf.CFDataGetLength.restype = ctypes.c_long
cf.CFDataGetBytePtr.argtypes = [ctypes.c_void_p]
cf.CFDataGetBytePtr.restype = ctypes.c_void_p
arr = cg.CGWindowListCopyWindowInfo(1 | 16, 0)
data = cf.CFPropertyListCreateData(None, arr, 100, 0, None)
raw = ctypes.string_at(cf.CFDataGetBytePtr(data), cf.CFDataGetLength(data))
wins = [w for w in plistlib.loads(raw) if w.get("kCGWindowOwnerName") == "iTerm2"]
print(json.dumps(wins))
"""

PTY_RUNNER_SCRIPT = """import fcntl, json, os, select, struct, subprocess, sys, termios, time, tty

spec = json.loads(open(sys.argv[1], encoding="utf-8").read())
if spec.get("pre_banner"):
    sys.stdout.write(spec["pre_banner"])
    sys.stdout.flush()

rows, cols = 30, 100
try:
    packed = fcntl.ioctl(0, termios.TIOCGWINSZ, struct.pack("HHHH", 0, 0, 0, 0))
    rows, cols, _, _ = struct.unpack("HHHH", packed)
except OSError:
    pass

tmux_ver = None
if spec.get("tmux_sock"):
    p = subprocess.run([spec["tmux_bin"], "-S", spec["tmux_sock"], "display-message", "-p", "tmux #{version}"], capture_output=True, text=True)
    if p.returncode == 0:
        tmux_ver = p.stdout.strip()

with open(spec["meta_path"], "w", encoding="utf-8") as mf:
    json.dump({"rows": rows or 30, "cols": cols or 100, "tmuxVersion": tmux_ver}, mf)

proc = subprocess.run([spec["program_path"], *spec["program_args"]])
exit_code = proc.returncode if proc.returncode >= 0 else 128 + (-proc.returncode)
sig_num = -proc.returncode if proc.returncode < 0 else 0

decrqm_raw = ""
if spec.get("query_decrqm", True):
    try:
        fd = os.open("/dev/tty", os.O_RDWR)
        old_attr = termios.tcgetattr(fd)
        try:
            tty.setraw(fd)
            os.write(fd, b"\\x1b[?1000$p\\x1b[?1002$p\\x1b[?1003$p\\x1b[?1006$p\\x1b[?2004$p\\x1b[?1004$p\\x1b[c")
            buf = b""
            deadline = time.monotonic() + 2.0
            while time.monotonic() < deadline:
                ready_r, _, _ = select.select([fd], [], [], 0.05)
                if ready_r:
                    buf += os.read(fd, 1024)
                    if buf.endswith(b"c") and b"$y" in buf:
                        break
            decrqm_raw = buf.decode("latin1")
        finally:
            termios.tcsetattr(fd, termios.TCSADRAIN, old_attr)
            os.close(fd)
    except OSError:
        pass

tmux_buf = None
if spec.get("tmux_sock"):
    pb = subprocess.run([spec["tmux_bin"], "-S", spec["tmux_sock"], "show-buffer"], capture_output=True, text=True)
    tmux_buf = pb.stdout if pb.returncode == 0 else None

with open(spec["exit_path"], "w", encoding="utf-8") as ef:
    json.dump({"exitCode": exit_code, "signal": sig_num, "decrqm": decrqm_raw, "tmuxBuffer": tmux_buf}, ef)
"""


@dataclass(frozen=True)
class GuestDefaultsBackup:
    """Tracks original guest iTerm2 plist, scroll direction, and Sequoia desktop-click state."""

    plist_backup_guest_path: str
    had_swipe_direction: bool
    swipe_direction_bool: bool
    had_click_show_desktop: bool = False
    click_show_desktop_bool: bool = True


@dataclass(frozen=True)
class SessionLaunchSpec:
    """Configuration for a single iTerm2 session inside the guest VM."""

    session_id: str
    guest_program_path: str
    program_args: list[str]
    tmux_mode: str | None = None
    guest_tmux_conf_path: str | None = None
    pre_banner: str | None = None
    cols: int = DEFAULT_COLS
    rows: int = DEFAULT_ROWS
    option_sends: int = 0
    allow_clipboard_access: bool = False
    copy_selection: bool = True


@dataclass(frozen=True)
class GuestItermSession:
    """Paths and handles for an active iTerm2 session sharing logs via VirtioFS."""

    session_id: str
    host_log_path: Path
    host_meta_path: Path
    host_exit_path: Path
    tmux_mode: str | None
    guest_tmux_sock: str | None


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
    swipe_proc = vm.exec_guest("defaults read -g com.apple.swipescrolldirection", check=False)
    had_swipe = swipe_proc.returncode == 0
    swipe_val = swipe_proc.stdout.strip() in ("1", "true", "TRUE", "YES") if had_swipe else True
    desk_proc = vm.exec_guest(
        "defaults read com.apple.WindowManager EnableStandardClickToShowDesktop", check=False
    )
    had_desk = desk_proc.returncode == 0
    desk_val = desk_proc.stdout.strip() in ("1", "true", "TRUE", "YES") if had_desk else True
    vm.exec_guest(
        f"defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool false && "
        f"{ACTIVATE_SETTINGS_BIN} -u 2>/dev/null || true",
        check=False,
    )
    return GuestDefaultsBackup(backup_path, had_swipe, swipe_val, had_desk, desk_val)


def restore_guest_defaults(vm: TartVmSession, backup: GuestDefaultsBackup) -> None:
    """Restores guest iTerm2 preferences, scroll setting, and WindowManager state."""
    swipe_restore = (
        f"defaults write -g com.apple.swipescrolldirection -bool {'true' if backup.swipe_direction_bool else 'false'}"
        if backup.had_swipe_direction
        else "defaults delete -g com.apple.swipescrolldirection 2>/dev/null || true"
    )
    desk_restore = (
        f"defaults write com.apple.WindowManager EnableStandardClickToShowDesktop -bool {'true' if backup.click_show_desktop_bool else 'false'}"
        if backup.had_click_show_desktop
        else "defaults delete com.apple.WindowManager EnableStandardClickToShowDesktop 2>/dev/null || true"
    )
    cmd = "\n".join(
        [
            "killall -9 iTerm2 2>/dev/null || true",
            "killall -9 iTermServer 2>/dev/null || true",
            f"test -f {backup.plist_backup_guest_path} && cp {backup.plist_backup_guest_path} ~/Library/Preferences/com.googlecode.iterm2.plist || true",
            swipe_restore,
            desk_restore,
            "killall cfprefsd 2>/dev/null || true",
            f"{ACTIVATE_SETTINGS_BIN} -u 2>/dev/null || true",
        ]
    )
    vm.exec_guest(cmd, check=False)


def set_guest_natural_scrolling(vm: TartVmSession, enabled: bool) -> None:
    """Sets macOS global `com.apple.swipescrolldirection` and activates it immediately."""
    flag = "true" if enabled else "false"
    cmd = f"defaults write -g com.apple.swipescrolldirection -bool {flag} && {ACTIVATE_SETTINGS_BIN} -u"
    vm.exec_guest(cmd)


ITERM_PREFS_UPDATER_SCRIPT = f"""import json, os, plistlib
cfg = json.load(open("{GUEST_SHARED_ROOT}/_iterm_prefs_cfg.json", encoding="utf-8"))
plist_path = os.path.expanduser("~/Library/Preferences/com.googlecode.iterm2.plist")
with open(plist_path, "rb") as fp:
    data = plistlib.load(fp)
data.update({{
    "SUEnableAutomaticChecks": False, "SUHasLaunchedBefore": True, "PromptOnQuit": False,
    "OnlyWhenMoreTabs": False, "NSQuitAlwaysKeepsWindows": False, "EnableRendezvous": False,
    "OpenArrangementAtStartup": False, "OpenNoWindowsAtStartup": True,
    "AutoHideTmuxClientSession": True, "AllowClipboardAccess": bool(cfg["allow_clipboard"]),
    "CopySelection": bool(cfg["copy_selection"]),
}})
for bm in data.get("New Bookmarks", []):
    bm["Columns"], bm["Rows"] = int(cfg["cols"]), int(cfg["rows"])
    bm["Option Key Sends"] = int(cfg["option_sends"])
    bm["Right Option Key Sends"] = int(cfg["option_sends"])
with open(plist_path, "wb") as fp:
    plistlib.dump(data, fp)
"""


def configure_iterm_preferences(vm: TartVmSession, spec: SessionLaunchSpec) -> None:
    """Terminates stale iTerm2 processes and writes deterministic session preferences."""
    (vm.shared_dir / "_iterm_prefs_updater.py").write_text(ITERM_PREFS_UPDATER_SCRIPT, encoding="utf-8")
    prefs_cfg = {
        "cols": spec.cols, "rows": spec.rows, "option_sends": spec.option_sends,
        "allow_clipboard": spec.allow_clipboard_access, "copy_selection": spec.copy_selection,
    }
    (vm.shared_dir / "_iterm_prefs_cfg.json").write_text(json.dumps(prefs_cfg), encoding="utf-8")
    vm.exec_guest(
        "\n".join([
            "killall -9 iTerm2 2>/dev/null || true",
            "killall -9 iTermServer 2>/dev/null || true",
            "rm -rf ~/Library/Saved\\ Application\\ State/com.googlecode.iterm2.savedState",
            "rm -rf ~/Library/Application\\ Support/iTerm2/SavedState",
            f'python3 "{GUEST_SHARED_ROOT}/_iterm_prefs_updater.py"',
            "killall cfprefsd 2>/dev/null || true",
        ])
    )


def _build_launch_command_body(spec: SessionLaunchSpec, guest_spec_json: str, guest_sock: str | None) -> str:
    runner_invocation = f'python3 "{GUEST_SHARED_ROOT}/_pty_runner.py" {shlex.quote(guest_spec_json)}'
    if spec.tmux_mode is None:
        return f"#!/bin/bash\nexport LANG=en_US.UTF-8\nexec {runner_invocation}\n"
    assert guest_sock is not None and spec.guest_tmux_conf_path is not None
    cc_flag = "-CC " if spec.tmux_mode.startswith("cc-") else ""
    quoted_runner = shlex.quote(runner_invocation)
    quoted_conf = shlex.quote(spec.guest_tmux_conf_path)
    return "\n".join([
        "#!/bin/bash",
        "export LANG=en_US.UTF-8",
        f"/opt/homebrew/bin/tmux -S {shlex.quote(guest_sock)} -f {quoted_conf} {cc_flag}new-session -x {spec.cols} -y {spec.rows} {quoted_runner}",
        f"/opt/homebrew/bin/tmux -S {shlex.quote(guest_sock)} kill-server 2>/dev/null || true",
        "",
    ])


def launch_iterm_session(vm: TartVmSession, spec: SessionLaunchSpec) -> GuestItermSession:
    """Configures iTerm2 preferences and opens a `.command` wrapper for `spec`."""
    ensure_guest_helper_scripts(vm)
    if not vm.env.get("RIG_ITERM2_MOCK_GUEST"):
        configure_iterm_preferences(vm, spec)
    sid = spec.session_id
    host_log = vm.shared_dir / f"{sid}.jsonl"
    host_meta = vm.shared_dir / f"{sid}.meta.json"
    host_exit = vm.shared_dir / f"{sid}.exit.json"
    for stale in (host_log, Path(f"{host_log}.ready"), host_meta, host_exit):
        stale.unlink(missing_ok=True)
    guest_log = f"{GUEST_SHARED_ROOT}/{sid}.jsonl"
    guest_sock = f"/tmp/tuikit-{sid}.sock" if spec.tmux_mode is not None else None
    runner_spec = {
        "program_path": spec.guest_program_path,
        "program_args": ["--log", guest_log, *spec.program_args],
        "meta_path": f"{GUEST_SHARED_ROOT}/{sid}.meta.json",
        "exit_path": f"{GUEST_SHARED_ROOT}/{sid}.exit.json",
        "pre_banner": spec.pre_banner,
        "tmux_sock": guest_sock,
        "tmux_bin": "/opt/homebrew/bin/tmux",
        "query_decrqm": True,
    }
    (vm.shared_dir / f"{sid}.spec.json").write_text(json.dumps(runner_spec), encoding="utf-8")
    guest_spec_path = f"{GUEST_SHARED_ROOT}/{sid}.spec.json"
    cmd_body = _build_launch_command_body(spec, guest_spec_path, guest_sock)
    host_cmd_script = vm.shared_dir / f"{sid}.command"
    host_cmd_script.write_text(cmd_body, encoding="utf-8")
    vm.exec_guest(
        f'cp "{GUEST_SHARED_ROOT}/{sid}.command" /tmp/tuikit_{sid}.command && '
        f"chmod +x /tmp/tuikit_{sid}.command && "
        f"# MOCK_ITERM2_EXEC_HOOK {guest_log}\n"
        f"open -a /Applications/iTerm.app /tmp/tuikit_{sid}.command"
    )
    return GuestItermSession(sid, host_log, host_meta, host_exit, spec.tmux_mode, guest_sock)


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
    return any(int(win.get("kCGWindowLayer", 0)) > 0 for win in windows)  # type: ignore[arg-type]


def _is_terminal_grid_window(win: dict[str, object]) -> bool:
    if int(win.get("kCGWindowLayer", -1)) != 0:  # type: ignore[arg-type]
        return False
    bounds = win.get("kCGWindowBounds")
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
    normal = [win for win in windows if _is_terminal_grid_window(win)]
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
    out = vm.exec_guest('lsappinfo info -only bundleid -only name "$(lsappinfo front)"', 10.0, False).stdout
    if "com.googlecode.iterm2" in out or '"iTerm2"' in out:
        return "iTerm2"
    return out.strip()


def _resolve_live_pane_grid(
    vm: TartVmSession, session: GuestItermSession, records: Sequence[dict[str, object]]
) -> tuple[int, int]:
    cols, rows = DEFAULT_COLS, DEFAULT_ROWS
    if session.host_meta_path.is_file():
        meta = json.loads(session.host_meta_path.read_text(encoding="utf-8"))
        cols, rows = int(meta.get("cols", DEFAULT_COLS)), int(meta.get("rows", DEFAULT_ROWS))
    for rec in records:
        if rec.get("kind") == "event" and rec.get("event_type") == "resize":
            cols, rows = int(rec.get("columns", cols)), int(rec.get("rows", rows))  # type: ignore[arg-type]
    if session.guest_tmux_sock and not vm.env.get("RIG_ITERM2_MOCK_GUEST"):
        q_sock = shlex.quote(session.guest_tmux_sock)
        out = vm.exec_guest(f"/opt/homebrew/bin/tmux -S {q_sock} display-message -p '#{{pane_width}}:#{{pane_height}}'", 5.0, False).stdout.strip()
        if ":" in out and all(part.isdigit() and int(part) > 0 for part in out.split(":", 1)):
            pw, ph = out.split(":", 1)
            cols, rows = int(pw), int(ph)
    return cols, rows


def resolve_session_geometry(
    vm: TartVmSession,
    session: GuestItermSession,
    timeout_seconds: float,
) -> WindowGeometry:
    """Combines live CoreGraphics window bounds with outer PTY grid dimensions and cell size."""
    active_win = poll_until(
        lambda: next((w for w in query_iterm_windows(vm) if _is_terminal_grid_window(w)), None),
        timeout_seconds,
        "on-screen iTerm2 window bounds",
    )
    bounds = active_win["kCGWindowBounds"]  # type: ignore[index]
    records = read_complete_jsonl_records(session.host_log_path)
    cols, inner_rows = _resolve_live_pane_grid(vm, session, records)
    outer_rows = inner_rows + (1 if session.tmux_mode in ("stock", "full") else 0)
    cell_w, cell_h = DEFAULT_CELL_WIDTH_PT, DEFAULT_CELL_HEIGHT_PT
    for rec in records:
        if rec.get("kind") == "section" and rec.get("section") == "cell_size" and rec.get("answered"):
            cell_w = float(rec.get("width_px", DEFAULT_CELL_WIDTH_PT))  # type: ignore[arg-type]
            cell_h = float(rec.get("height_px", DEFAULT_CELL_HEIGHT_PT))  # type: ignore[arg-type]
    win_x, win_y = float(bounds["X"]), float(bounds["Y"])  # type: ignore[index]
    win_w, win_h = float(bounds["Width"]), float(bounds["Height"])  # type: ignore[index]
    cell_w, cell_h = normalize_cell_size_pt(cell_w, cell_h, win_w, win_h, cols, outer_rows, 2.0)
    return WindowGeometry(win_x, win_y, win_w, win_h, cols, outer_rows, cell_w, cell_h, 2.0)
