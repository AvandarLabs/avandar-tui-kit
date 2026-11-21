"""Tart macOS VM lifecycle management, loopback VNC handshake, and guest command execution."""

from __future__ import annotations

import atexit
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time

from driver import DEFAULT_VM_NAME
from driver.cli_args import DriverRuntimeError
from driver.wait_helpers import poll_until

# Pattern matching Tart's `vnc://:<password>@<host>:<port>` loopback VNC line.
VNC_URL_PATTERN = re.compile(r"vnc://:([^@\s]+)@([^:\s]+):(\d+)")

# Guest mount point for the `tuikit` VirtioFS shared folder.
GUEST_SHARED_ROOT = "/Volumes/My Shared Files/tuikit"

# Recovery installation hints for required host tools on macOS.
TOOL_INSTALL_HINTS: dict[str, str] = {
    "tart": "Install Tart via 'brew install cirruslabs/cli/tart' and ensure 'tart' is in PATH.",
    "vncdo": "Install vncdotool via 'uv tool install vncdotool' and ensure 'vncdo' is in PATH.",
    "cargo": "Install Rust via rustup and ensure '~/.cargo/bin' is in PATH.",
}


@dataclass(frozen=True)
class VncEndpoint:
    """Loopback VNC server connection parameters emitted by `tart run --vnc-experimental`."""

    host: str
    port: int
    password: str


def parse_vnc_connection_url(text: str) -> VncEndpoint | None:
    """Extracts `VncEndpoint` from Tart's `vnc://:<password>@<host>:<port>` output line."""
    match = VNC_URL_PATTERN.search(text)
    if match is None:
        return None
    return VncEndpoint(host=match.group(2), port=int(match.group(3)), password=match.group(1))


def _default_fallback_dirs(tool_name: str, env: Mapping[str, str]) -> list[str]:
    directories: list[str] = []
    home = env.get("HOME", "")
    if tool_name == "vncdo" and home:
        directories.append(str(Path(home) / ".local" / "bin"))
    if tool_name in ("tart", "vncdo"):
        directories.extend(["/opt/homebrew/bin", "/usr/local/bin"])
    return directories


def resolve_host_binary(
    tool_name: str,
    env: Mapping[str, str] | None = None,
    fallback_dirs: Sequence[str] | None = None,
) -> str:
    """Finds an executable host tool in override env, PATH, or non-strict standard locations."""
    active_env = os.environ if env is None else env
    override_key = f"RIG_ITERM2_{tool_name.upper()}_BIN"
    override_value = active_env.get(override_key, "").strip()
    if override_value:
        resolved = Path(override_value).expanduser().resolve()
        if resolved.is_file() and os.access(resolved, os.X_OK):
            return str(resolved)
        raise DriverRuntimeError(
            f"{override_key} does not point to an executable binary: '{override_value}'.",
            f"Unset {override_key} or point it to a valid '{tool_name}' executable.",
        )
    found_in_path = shutil.which(tool_name, path=active_env.get("PATH", ""))
    if found_in_path:
        return found_in_path
    if not active_env.get("RIG_ITERM2_STRICT_PATH"):
        candidates = (
            list(fallback_dirs)
            if fallback_dirs is not None
            else _default_fallback_dirs(tool_name, active_env)
        )
        for directory in candidates:
            candidate = Path(directory) / tool_name
            if candidate.is_file() and os.access(candidate, os.X_OK):
                return str(candidate)
    hint = TOOL_INSTALL_HINTS.get(tool_name, f"Ensure '{tool_name}' is installed and available in PATH.")
    raise DriverRuntimeError(f"Could not find required host tool '{tool_name}' in PATH.", hint)


def check_rfb_port_ready(endpoint: VncEndpoint) -> bool:
    """Verifies that `endpoint` accepts a TCP connection and speaks the `RFB ` greeting."""
    try:
        with socket.create_connection((endpoint.host, endpoint.port), timeout=1.0) as rfb_socket:
            rfb_socket.settimeout(1.0)
            return rfb_socket.recv(12).startswith(b"RFB ")
    except OSError:
        return False


def _is_vm_disk_unlocked(vm_name: str, env: Mapping[str, str]) -> bool:
    home = env.get("HOME", "")
    if not home:
        return True
    disk_path = Path(home) / ".tart" / "vms" / vm_name / "disk.img"
    if not disk_path.is_file():
        return True
    disk_fd = os.open(disk_path, os.O_RDWR)
    try:
        fcntl.flock(disk_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(disk_fd, fcntl.LOCK_UN)
        return True
    except OSError:
        return False
    finally:
        os.close(disk_fd)


def wait_until_vm_fully_stopped(tart_bin: str, vm_name: str, env: Mapping[str, str]) -> None:
    """Waits until Tart reports `stopped` and Virtualization.framework releases `disk.img`."""

    def _check_stopped() -> bool | None:
        completed = subprocess.run(
            [tart_bin, "list", "--format", "json"],
            capture_output=True, text=True, env=dict(env), timeout=15, check=False,
        )
        if completed.returncode != 0:
            return None
        entries = [entry for entry in json.loads(completed.stdout) if entry.get("Name") == vm_name]
        if any(entry.get("State") == "running" for entry in entries):
            return None
        return True if _is_vm_disk_unlocked(vm_name, env) else None

    poll_until(_check_stopped, 20.0, f"VM '{vm_name}' stop and disk lock release", 0.2)


def _run_tart_host_command(
    command: Sequence[str], env: Mapping[str, str], timeout_seconds: float, action_label: str,
) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            list(command), capture_output=True, text=True, env=dict(env), timeout=timeout_seconds, check=False,
        )
    except subprocess.TimeoutExpired as timeout_error:
        raise DriverRuntimeError(
            f"'{action_label}' timed out after {timeout_seconds:.1f}s.",
            "Verify Tart is responsive by running 'tart list' manually.",
        ) from timeout_error
    except OSError as os_error:
        raise DriverRuntimeError(
            f"Failed to execute '{action_label}': {os_error}",
            "Verify Tart is installed and executable in PATH.",
        ) from os_error


def ensure_vm_present_and_cold(tart_bin: str, vm_name: str, env: Mapping[str, str]) -> None:
    """Verifies `vm_name` exists in `tart list --format json` and stops it if already running."""
    completed = _run_tart_host_command([tart_bin, "list", "--format", "json"], env, 15.0, "tart list")
    if completed.returncode != 0:
        raise DriverRuntimeError(
            f"'tart list' failed with exit code {completed.returncode}: {completed.stderr.strip()}",
            "Verify Tart installation and check 'tart list' manually.",
        )
    try:
        entries = json.loads(completed.stdout)
    except json.JSONDecodeError as decode_error:
        raise DriverRuntimeError(
            f"Could not parse 'tart list --format json' output: {decode_error}",
            "Upgrade Tart to >= 2.42 and verify 'tart list --format json'.",
        ) from decode_error
    matching = [item for item in entries if item.get("Name") == vm_name]
    if not matching:
        raise DriverRuntimeError(
            f"Prepared Tart VM '{vm_name}' was not found.",
            f"Create and prepare '{vm_name}' following docs/testing.md before running Layer 6.",
        )
    if any(item.get("State") == "running" for item in matching):
        _run_tart_host_command([tart_bin, "stop", vm_name], env, 30.0, f"tart stop {vm_name}")
    wait_until_vm_fully_stopped(tart_bin, vm_name, env)


class TartVmSession:
    """Manages a single cold-booted headless `tuikit-macos` VM with VirtioFS and loopback VNC."""

    def __init__(
        self, tart_bin: str, vm_name: str = DEFAULT_VM_NAME, env: Mapping[str, str] | None = None,
    ) -> None:
        self.tart_bin = tart_bin
        self.vm_name = vm_name
        self.env = dict(os.environ if env is None else env)
        self.shared_dir = Path(os.path.realpath(tempfile.mkdtemp(prefix="tuikit-iterm2-")))
        self._process: subprocess.Popen[str] | None = None
        self._endpoint: VncEndpoint | None = None
        self._boot_lines: list[str] = []
        self._is_stopped = False
        self._has_started_vm = False
        self._previous_signal_handlers: dict[int, object] = {}

    @property
    def vnc_endpoint(self) -> VncEndpoint:
        """Returns the discovered loopback VNC endpoint for the booted VM."""
        if self._endpoint is None:
            raise DriverRuntimeError("VNC endpoint requested before VM boot.", "Call boot() first.")
        return self._endpoint

    def _register_cleanup_hooks(self) -> None:
        atexit.register(self.stop)
        for signal_number in (signal.SIGINT, signal.SIGTERM):
            try:
                self._previous_signal_handlers[signal_number] = signal.getsignal(signal_number)

                def _handle_signal(received_signal: int, _frame: object) -> None:
                    self.stop()
                    raise SystemExit(128 + received_signal)

                signal.signal(signal_number, _handle_signal)
            except ValueError:
                pass

    def _restore_signal_handlers(self) -> None:
        for signal_number, previous_handler in list(self._previous_signal_handlers.items()):
            try:
                signal.signal(signal_number, previous_handler)  # type: ignore[arg-type]
            except (OSError, ValueError):
                pass
        self._previous_signal_handlers.clear()

    def _consume_tart_stdout(self, pipe: object) -> None:
        assert pipe is not None
        for line in pipe:  # type: ignore[attr-defined]
            self._boot_lines.append(line.rstrip("\n"))
            parsed = parse_vnc_connection_url(line)
            if parsed is not None:
                self._endpoint = parsed

    def _reap_tart_process(self, should_terminate_immediately: bool = False) -> None:
        process = self._process
        self._process = None
        if process is None:
            return
        if not should_terminate_immediately:
            try:
                process.wait(timeout=5)
                return
            except subprocess.TimeoutExpired:
                pass
        try:
            process.terminate()
            process.wait(timeout=2)
            return
        except (subprocess.TimeoutExpired, OSError):
            pass
        try:
            process.kill()
            process.wait(timeout=5)
        except (subprocess.TimeoutExpired, OSError):
            pass

    def _spawn_tart_run(self) -> None:
        self._endpoint = None
        self._boot_lines.clear()
        self._has_started_vm = True
        command = [
            self.tart_bin, "run", "--no-graphics", "--no-audio", "--no-clipboard",
            "--vnc-experimental", f"--dir=tuikit:{self.shared_dir}", self.vm_name,
        ]
        try:
            self._process = subprocess.Popen(
                command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=self.env
            )
        except OSError as os_error:
            raise DriverRuntimeError(
                f"Failed to start 'tart run': {os_error}", "Verify Tart is installed and executable."
            ) from os_error
        threading.Thread(
            target=self._consume_tart_stdout, args=(self._process.stdout,), daemon=True
        ).start()

    def boot(self, timeout_seconds: float) -> VncEndpoint:
        """Cold-boots `vm_name` headless and waits for both RFB and VirtioFS/GUI readiness."""
        ensure_vm_present_and_cold(self.tart_bin, self.vm_name, self.env)
        self._register_cleanup_hooks()
        deadline = time.monotonic() + timeout_seconds
        try:
            for attempt in range(3):
                self._reap_tart_process(should_terminate_immediately=True)
                self._spawn_tart_run()
                try:
                    return self._wait_until_boot_ready(max(deadline - time.monotonic(), 5.0))
                except DriverRuntimeError as boot_error:
                    if attempt < 2 and "VZErrorDomain" in boot_error.message and time.monotonic() < deadline:
                        wait_until_vm_fully_stopped(self.tart_bin, self.vm_name, self.env)
                        continue
                    raise
            raise DriverRuntimeError("Exhausted VM boot attempts.", "Check Virtualization.framework.")
        except Exception:
            self.stop()
            raise

    def _wait_until_boot_ready(self, timeout_seconds: float) -> VncEndpoint:
        readiness_probe = (
            f'pgrep -x Dock >/dev/null && pgrep -x Finder >/dev/null && test -d "{GUEST_SHARED_ROOT}"'
        )

        def _is_ready() -> VncEndpoint | None:
            if self._process is not None and self._process.poll() is not None:
                tail = " | ".join(self._boot_lines[-5:])
                raise DriverRuntimeError(
                    f"'tart run' exited prematurely (code {self._process.returncode}): {tail}",
                    "Ensure macOS Virtualization entitlements are available and at most 2 macOS VMs run.",
                )
            if self._endpoint is None or not check_rfb_port_ready(self._endpoint):
                return None
            if self.exec_guest(readiness_probe, timeout_seconds=8.0, should_check=False).returncode == 0:
                return self._endpoint
            return None

        return poll_until(_is_ready, timeout_seconds, f"VM '{self.vm_name}' VNC + guest desktop readiness", 0.25)

    def exec_guest(
        self, shell_command: str, timeout_seconds: float = 25.0, should_check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        """Executes `shell_command` inside the macOS guest via `tart exec`."""
        try:
            result = subprocess.run(
                [self.tart_bin, "exec", self.vm_name, "bash", "-lc", shell_command],
                capture_output=True, text=True, env=self.env, timeout=timeout_seconds, check=False,
            )
        except subprocess.TimeoutExpired:
            result = subprocess.CompletedProcess([], 124, "", "tart exec timed out")
        except OSError as os_error:
            raise DriverRuntimeError(
                f"Failed to invoke 'tart exec': {os_error}", "Verify Tart is installed and executable."
            ) from os_error
        if should_check and result.returncode != 0:
            raise DriverRuntimeError(
                f"Guest command failed (code {result.returncode}): {result.stderr.strip() or result.stdout.strip()}",
                "Verify guest tools inside tuikit-macos via 'tart exec tuikit-macos bash -lc id'.",
            )
        return result

    def stage_host_file(self, source_path: Path, filename: str, is_executable: bool = False) -> Path:
        """Copies a host file into the VirtioFS shared directory."""
        destination = self.shared_dir / filename
        shutil.copy2(source_path, destination)
        if is_executable:
            os.chmod(destination, 0o755)
        return destination

    def read_guest_clipboard(self) -> str:
        """Reads the macOS guest pasteboard via `LANG=en_US.UTF-8 pbpaste`."""
        return self.exec_guest("LANG=en_US.UTF-8 pbpaste", timeout_seconds=10.0).stdout

    def write_guest_clipboard(self, content: str) -> None:
        """Writes UTF-8 `content` into the macOS guest pasteboard via a shared file."""
        clipboard_file = self.shared_dir / "_guest_clipboard_payload.txt"
        clipboard_file.write_text(content, encoding="utf-8")
        self.exec_guest(f'LANG=en_US.UTF-8 pbcopy < "{GUEST_SHARED_ROOT}/_guest_clipboard_payload.txt"', 10.0)

    def _stop_active_vm(self) -> None:
        did_tart_stop = False
        try:
            subprocess.run(
                [self.tart_bin, "stop", self.vm_name],
                capture_output=True, text=True, env=self.env, timeout=30, check=False,
            )
            did_tart_stop = True
        except (subprocess.TimeoutExpired, OSError):
            pass
        self._reap_tart_process(should_terminate_immediately=not did_tart_stop)
        if did_tart_stop:
            try:
                wait_until_vm_fully_stopped(self.tart_bin, self.vm_name, self.env)
            except DriverRuntimeError:
                pass

    def stop(self) -> None:
        """Idempotently stops `vm_name`, reaps `tart run`, and removes temporary shared files."""
        if self._is_stopped:
            return
        self._is_stopped = True
        atexit.unregister(self.stop)
        self._restore_signal_handlers()
        try:
            if self._has_started_vm or self._process is not None:
                self._stop_active_vm()
        finally:
            shutil.rmtree(self.shared_dir, ignore_errors=True)
