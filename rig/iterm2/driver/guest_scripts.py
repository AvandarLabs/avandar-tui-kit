"""Embedded macOS guest Python scripts staged over VirtioFS for CoreGraphics, PTY, and plist updates."""

from __future__ import annotations

from driver.tart_vm import GUEST_SHARED_ROOT

ACTIVATE_SETTINGS_BIN = (
    "/System/Library/PrivateFrameworks/SystemAdministration.framework/Resources/activateSettings"
)

CG_WINDOWS_SCRIPT = """import ctypes, json, plistlib
core_graphics = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
core_foundation = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
core_graphics.CGWindowListCopyWindowInfo.argtypes = [ctypes.c_uint32, ctypes.c_uint32]
core_graphics.CGWindowListCopyWindowInfo.restype = ctypes.c_void_p
core_foundation.CFPropertyListCreateData.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_long, ctypes.c_ulong, ctypes.c_void_p]
core_foundation.CFPropertyListCreateData.restype = ctypes.c_void_p
core_foundation.CFDataGetLength.argtypes = [ctypes.c_void_p]
core_foundation.CFDataGetLength.restype = ctypes.c_long
core_foundation.CFDataGetBytePtr.argtypes = [ctypes.c_void_p]
core_foundation.CFDataGetBytePtr.restype = ctypes.c_void_p
window_array = core_graphics.CGWindowListCopyWindowInfo(1 | 16, 0)
plist_data = core_foundation.CFPropertyListCreateData(None, window_array, 100, 0, None)
raw_bytes = ctypes.string_at(core_foundation.CFDataGetBytePtr(plist_data), core_foundation.CFDataGetLength(plist_data))
iterm_windows = [entry for entry in plistlib.loads(raw_bytes) if entry.get("kCGWindowOwnerName") == "iTerm2"]
print(json.dumps(iterm_windows))
"""

PTY_RUNNER_SCRIPT = """import fcntl, json, os, select, struct, subprocess, sys, termios, time, tty

spec = json.loads(open(sys.argv[1], encoding="utf-8").read())
if spec.get("wait_cc_attach", False):
    try:
        tty_fd = os.open("/dev/tty", os.O_RDWR)
        old_attr = termios.tcgetattr(tty_fd)
        try:
            tty.setraw(tty_fd)
            deadline = time.monotonic() + 5.0
            while time.monotonic() < deadline:
                os.write(tty_fd, b"\\x1b]11;?\\x07\\x1b[c")
                reply_buffer = b""
                query_limit = time.monotonic() + 0.5
                while time.monotonic() < query_limit:
                    readable_fds, _, _ = select.select([tty_fd], [], [], 0.05)
                    if readable_fds:
                        reply_buffer += os.read(tty_fd, 1024)
                        if reply_buffer.endswith(b"c") and b"\\x1b[?" in reply_buffer:
                            break
                if b"rgb:" in reply_buffer and b"rgb:0000/0000/0000" not in reply_buffer:
                    break
                extra_fds, _, _ = select.select([tty_fd], [], [], 0.02)
                if extra_fds:
                    extra_bytes = os.read(tty_fd, 1024)
                    if b"rgb:" in extra_bytes and b"rgb:0000/0000/0000" not in extra_bytes:
                        break
            termios.tcflush(tty_fd, termios.TCIFLUSH)
        finally:
            termios.tcsetattr(tty_fd, termios.TCSADRAIN, old_attr)
            os.close(tty_fd)
    except OSError:
        pass

if spec.get("pre_banner"):
    sys.stdout.write(spec["pre_banner"])
    sys.stdout.flush()

rows, cols = 30, 100
try:
    packed = fcntl.ioctl(0, termios.TIOCGWINSZ, struct.pack("HHHH", 0, 0, 0, 0))
    rows, cols, _, _ = struct.unpack("HHHH", packed)
except OSError:
    pass

tmux_version = None
if spec.get("tmux_sock"):
    version_proc = subprocess.run([spec["tmux_bin"], "-S", spec["tmux_sock"], "display-message", "-p", "tmux #{version}"], capture_output=True, text=True)
    if version_proc.returncode == 0:
        tmux_version = version_proc.stdout.strip()

with open(spec["meta_path"], "w", encoding="utf-8") as meta_file:
    json.dump({"rows": rows or 30, "cols": cols or 100, "tmuxVersion": tmux_version}, meta_file)

proc = subprocess.run([spec["program_path"], *spec["program_args"]])
exit_code = proc.returncode if proc.returncode >= 0 else 128 + (-proc.returncode)
sig_num = -proc.returncode if proc.returncode < 0 else 0

decrqm_raw = ""
if spec.get("query_decrqm", True):
    try:
        tty_fd = os.open("/dev/tty", os.O_RDWR)
        old_attr = termios.tcgetattr(tty_fd)
        try:
            tty.setraw(tty_fd)
            os.write(tty_fd, b"\\x1b[?1000$p\\x1b[?1002$p\\x1b[?1003$p\\x1b[?1006$p\\x1b[?2004$p\\x1b[?1004$p\\x1b[c")
            reply_buffer = b""
            deadline = time.monotonic() + 2.0
            while time.monotonic() < deadline:
                readable_fds, _, _ = select.select([tty_fd], [], [], 0.05)
                if readable_fds:
                    reply_buffer += os.read(tty_fd, 1024)
                    if reply_buffer.endswith(b"c") and b"$y" in reply_buffer:
                        break
            decrqm_raw = reply_buffer.decode("latin1")
        finally:
            termios.tcsetattr(tty_fd, termios.TCSADRAIN, old_attr)
            os.close(tty_fd)
    except OSError:
        pass

tmux_buffer = None
if spec.get("tmux_sock"):
    buffer_proc = subprocess.run([spec["tmux_bin"], "-S", spec["tmux_sock"], "show-buffer"], capture_output=True, text=True)
    tmux_buffer = buffer_proc.stdout if buffer_proc.returncode == 0 else None

with open(spec["exit_path"], "w", encoding="utf-8") as exit_file:
    json.dump({"exitCode": exit_code, "signal": sig_num, "decrqm": decrqm_raw, "tmuxBuffer": tmux_buffer}, exit_file)
"""

ITERM_PREFS_UPDATER_SCRIPT = f"""import json, os, plistlib
config = json.load(open("{GUEST_SHARED_ROOT}/_iterm_prefs_cfg.json", encoding="utf-8"))
plist_path = os.path.expanduser("~/Library/Preferences/com.googlecode.iterm2.plist")
with open(plist_path, "rb") as plist_file:
    data = plistlib.load(plist_file)
data.update({{
    "SUEnableAutomaticChecks": False, "SUHasLaunchedBefore": True, "PromptOnQuit": False,
    "OnlyWhenMoreTabs": False, "NSQuitAlwaysKeepsWindows": False, "EnableRendezvous": False,
    "OpenArrangementAtStartup": False, "OpenNoWindowsAtStartup": True,
    "AutoHideTmuxClientSession": True, "AllowClipboardAccess": bool(config["allow_clipboard"]),
    "CopySelection": bool(config["copy_selection"]),
}})
for bookmark in data.get("New Bookmarks", []):
    bookmark["Columns"], bookmark["Rows"] = int(config["cols"]), int(config["rows"])
    bookmark["Option Key Sends"] = int(config["option_sends"])
    bookmark["Right Option Key Sends"] = int(config["option_sends"])
with open(plist_path, "wb") as plist_file:
    plistlib.dump(data, plist_file)
"""
