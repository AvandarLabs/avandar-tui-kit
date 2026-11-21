"""Unit tests for TartVmSession teardown safety, subprocess error wrapping, DECRQM, and pointer assertions."""

from __future__ import annotations

from collections.abc import Iterator
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

# Parent directory (`rig/iterm2`) added to `sys.path` for `driver` imports.
DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from driver.cli_args import DriverRuntimeError  # noqa: E402
from driver.interactive_steps import (  # noqa: E402
    assert_iterm2_pointer_results,
    run_focus_steps,
    wait_for_session_exit_and_teardown,
)
from driver.iterm_window import GuestItermSession  # noqa: E402
from driver.tart_vm import (  # noqa: E402
    TartVmSession,
    VncEndpoint,
    ensure_vm_present_and_cold,
)
from driver.vnc_input import VncController, WindowGeometry  # noqa: E402


class StubHungProcess:
    """Simulates a `tart run` child process that announces VNC and hangs on wait."""

    def __init__(self, lines: tuple[str, ...] = ("vnc://:pw@127.0.0.1:59001\n",)) -> None:
        self.stdout: Iterator[str] = iter(lines)
        self.returncode: int | None = None
        self.was_terminated = False
        self.was_killed = False
        self.wait_calls: list[float | None] = []

    def poll(self) -> int | None:
        return self.returncode

    def terminate(self) -> None:
        self.was_terminated = True

    def kill(self) -> None:
        self.was_killed = True

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        raise subprocess.TimeoutExpired(cmd=["tart", "run"], timeout=timeout or 1.0)


class StubPrematureExitProcess:
    """Simulates a `tart run` child process that emits diagnostic lines and exits."""

    def __init__(self, lines: tuple[str, ...], exit_code: int = 1) -> None:
        self._lines = lines
        self._has_drained_stdout = False
        self.stdout = self._iter_stdout()
        self.returncode = exit_code
        self.was_terminated = False

    def _iter_stdout(self) -> Iterator[str]:
        yield from self._lines
        self._has_drained_stdout = True

    def poll(self) -> int | None:
        return self.returncode if self._has_drained_stdout else None

    def terminate(self) -> None:
        self.was_terminated = True

    def kill(self) -> None:
        pass

    def wait(self, timeout: float | None = None) -> int:
        _ = timeout
        return self.returncode


def _fake_tart_run_dispatch(command: list[str], **_kwargs: object) -> subprocess.CompletedProcess[str]:
    if command[1:3] == ["list", "--format"]:
        payload = json.dumps([{"Name": "tuikit-macos", "State": "stopped"}])
        return subprocess.CompletedProcess(command, 0, payload, "")
    return subprocess.CompletedProcess(command, 0, "", "")


def build_valid_pointer_steps() -> dict[str, list[dict[str, object]]]:
    """Builds minimal valid pointer step event slices satisfying Rule M1 and interactive checks."""
    return {
        "hover": [],
        "left_click": [
            {"event_type": "mouse", "mouse_kind": "down:left", "column": 10, "row": 5},
            {"event_type": "mouse", "mouse_kind": "up:left", "column": 10, "row": 5},
        ],
        "left_drag": [
            {"event_type": "mouse", "mouse_kind": "down:left", "column": 12, "row": 6},
            {"event_type": "mouse", "mouse_kind": "drag:left", "column": 15, "row": 6},
            {"event_type": "mouse", "mouse_kind": "up:left", "column": 18, "row": 6},
        ],
        "wheel_down": [{"event_type": "mouse", "mouse_kind": "scroll_down", "column": 10, "row": 5}],
        "wheel_up": [{"event_type": "mouse", "mouse_kind": "scroll_up", "column": 10, "row": 5}],
    }


class TartVmLifecycleAndSubprocessTests(unittest.TestCase):
    def test_stop_terminates_and_kills_hung_tart_process_when_tart_stop_times_out(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        shared_dir = vm.shared_dir
        (shared_dir / "payload.txt").write_text("staged", encoding="utf-8")
        hung_process = StubHungProcess()
        with (
            mock.patch("driver.tart_vm.subprocess.Popen", return_value=hung_process),
            mock.patch("driver.tart_vm.subprocess.run", side_effect=_fake_tart_run_dispatch),
            mock.patch("driver.tart_vm.check_rfb_port_ready", return_value=True),
        ):
            endpoint = vm.boot(timeout_seconds=5.0)
        self.assertEqual(endpoint, VncEndpoint(host="127.0.0.1", port=59001, password="pw"))

        with mock.patch(
            "driver.tart_vm.subprocess.run",
            side_effect=subprocess.TimeoutExpired(cmd=["tart", "stop"], timeout=30.0),
        ):
            vm.stop()

        self.assertTrue(hung_process.was_terminated)
        self.assertTrue(hung_process.was_killed)
        self.assertFalse(shared_dir.exists())

    def test_stop_skips_tart_stop_when_boot_never_started_vm(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        shared_dir = vm.shared_dir
        self.assertTrue(shared_dir.is_dir())

        with mock.patch("driver.tart_vm.subprocess.run") as run_mock:
            vm.stop()

        run_mock.assert_not_called()
        self.assertFalse(shared_dir.exists())

    def test_boot_retries_transient_vz_error_and_reaps_failed_process(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        failed_process = StubPrematureExitProcess(("Error Domain=VZErrorDomain Code=2\n",), exit_code=1)
        ready_process = StubHungProcess(("vnc://:pw@127.0.0.1:59001\n",))
        with (
            mock.patch("driver.tart_vm.subprocess.Popen", side_effect=[failed_process, ready_process]) as popen_mock,
            mock.patch("driver.tart_vm.subprocess.run", side_effect=_fake_tart_run_dispatch),
            mock.patch("driver.tart_vm.check_rfb_port_ready", return_value=True),
        ):
            endpoint = vm.boot(timeout_seconds=10.0)
            self.assertEqual(endpoint, VncEndpoint(host="127.0.0.1", port=59001, password="pw"))
            self.assertEqual(popen_mock.call_count, 2)
            vm.stop()

    def test_subprocess_timeouts_and_oserrors_raise_driver_runtime_error(self) -> None:
        endpoint = VncEndpoint(host="127.0.0.1", port=59000, password="secret")
        controller = VncController("/usr/local/bin/vncdo", endpoint)

        for raised_error in (
            subprocess.TimeoutExpired(cmd=["vncdo"], timeout=5.0),
            FileNotFoundError("No such file or directory: vncdo"),
        ):
            with mock.patch("driver.vnc_input.subprocess.run", side_effect=raised_error):
                with self.assertRaises(DriverRuntimeError):
                    controller.run_tokens(["key", "f2"], timeout_seconds=5.0)

        for raised_error in (
            subprocess.TimeoutExpired(cmd=["tart", "list"], timeout=15.0),
            OSError("tart binary permission denied"),
        ):
            with mock.patch("driver.tart_vm.subprocess.run", side_effect=raised_error):
                with self.assertRaises(DriverRuntimeError):
                    ensure_vm_present_and_cold("/usr/local/bin/tart", "tuikit-macos", {})


class TeardownAndPointerAssertionTests(unittest.TestCase):
    def test_wait_for_session_exit_rejects_all_leaked_decrqm_modes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            base = Path(temp_dir)
            session = GuestItermSession(
                session_id="test_01",
                host_log_path=base / "test_01.jsonl",
                host_meta_path=base / "test_01.meta.json",
                host_exit_path=base / "test_01.exit.json",
                tmux_mode=None,
                guest_tmux_socket=None,
            )
            clean_decrqm = (
                "\x1b[?1000;2$y\x1b[?1002;2$y\x1b[?1003;2$y"
                "\x1b[?1006;2$y\x1b[?2004;2$y\x1b[?1004;2$y"
            )
            session.host_exit_path.write_text(
                json.dumps({"exitCode": 0, "signal": 0, "decrqm": clean_decrqm}),
                encoding="utf-8",
            )
            clean_outcome = wait_for_session_exit_and_teardown(session, 0.5)
            self.assertEqual(clean_outcome["exitCode"], 0)

            leaked_replies = [
                clean_decrqm.replace("?1002;2$y", "?1002;1$y"),
                clean_decrqm.replace("?1006;2$y", "?1006;1$y"),
                clean_decrqm.replace("?2004;2$y", "?2004;1$y"),
                clean_decrqm.replace("?1004;2$y", "?1004;1$y"),
            ]
            for leaked_reply in leaked_replies:
                session.host_exit_path.write_text(
                    json.dumps({"exitCode": 0, "signal": 0, "decrqm": leaked_reply}),
                    encoding="utf-8",
                )
                with self.assertRaises(DriverRuntimeError):
                    wait_for_session_exit_and_teardown(session, 0.5)

    def test_assert_iterm2_pointer_results_validates_m1_and_expected_events(self) -> None:
        valid_steps = build_valid_pointer_steps()
        assert_iterm2_pointer_results(valid_steps, None)

        hover_leak_steps = build_valid_pointer_steps()
        hover_leak_steps["hover"] = [{"event_type": "mouse", "mouse_kind": "moved", "column": 2, "row": 2}]
        with self.assertRaises(DriverRuntimeError) as m1_error:
            assert_iterm2_pointer_results(hover_leak_steps, None)
        self.assertIn("Rule M1", m1_error.exception.message)

        for missing_key in ("left_click", "left_drag", "wheel_down", "wheel_up"):
            incomplete_steps = build_valid_pointer_steps()
            incomplete_steps[missing_key] = []
            with self.assertRaises(DriverRuntimeError):
                assert_iterm2_pointer_results(incomplete_steps, "full")

        empty_cc_steps = {key: [] for key in valid_steps}
        for cc_mode in ("cc-stock", "cc-full"):
            with self.assertRaises(DriverRuntimeError):
                assert_iterm2_pointer_results(empty_cc_steps, cc_mode)

    def test_run_focus_steps_requires_focus_events_in_cc_modes(self) -> None:
        geometry = WindowGeometry(93.0, 168.0, 725.0, 543.0, 100, 30, 7.0, 17.0, 2.0)
        with tempfile.TemporaryDirectory() as temp_dir:
            log_path = Path(temp_dir) / "focus.jsonl"
            log_path.write_text("", encoding="utf-8")
            fake_vm, fake_vnc = mock.MagicMock(), mock.MagicMock()

            def append_fence() -> None:
                with log_path.open("a", encoding="utf-8") as handle:
                    handle.write('{"kind":"event","event_type":"key","key_kind":"press","code":"f2"}\n')

            fake_vnc.send_fence_key.side_effect = append_fence
            with (
                mock.patch("driver.interactive_steps.query_frontmost_app_name", return_value="Finder"),
                mock.patch("driver.interactive_steps.ensure_iterm_window_focused"),
            ):
                stock_session = GuestItermSession("s1", log_path, log_path, log_path, "stock", None)
                self.assertEqual(
                    run_focus_steps(fake_vm, fake_vnc, stock_session, geometry, 0.05),
                    {"focusLostEvents": [], "focusGainedEvents": []},
                )
                for cc_mode in ("cc-stock", "cc-full"):
                    cc_session = GuestItermSession("s2", log_path, log_path, log_path, cc_mode, None)
                    with self.assertRaises(DriverRuntimeError):
                        run_focus_steps(fake_vm, fake_vnc, cc_session, geometry, 0.05)

    def test_launch_iterm_session_enables_rule_5_cc_attach_wait_only_for_cc_modes(self) -> None:
        import ast
        from driver.guest_scripts import PTY_RUNNER_SCRIPT
        from driver.iterm_window import SessionLaunchSpec, launch_iterm_session

        ast.parse(PTY_RUNNER_SCRIPT)
        self.assertIn("wait_cc_attach", PTY_RUNNER_SCRIPT)
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        try:
            with mock.patch.object(vm, "exec_guest"):
                full_session = launch_iterm_session(
                    vm,
                    SessionLaunchSpec(
                        "s_full", "/tmp/probe", ["--batch"], tmux_mode="full", guest_tmux_conf_path="/tmp/f.conf"
                    ),
                )
                cc_session = launch_iterm_session(
                    vm,
                    SessionLaunchSpec(
                        "s_cc", "/tmp/probe", ["--batch"], tmux_mode="cc-full", guest_tmux_conf_path="/tmp/f.conf"
                    ),
                )
            full_spec = json.loads((vm.shared_dir / f"{full_session.session_id}.spec.json").read_text(encoding="utf-8"))
            cc_spec = json.loads((vm.shared_dir / f"{cc_session.session_id}.spec.json").read_text(encoding="utf-8"))
            self.assertIs(full_spec["wait_cc_attach"], False)
            self.assertIs(cc_spec["wait_cc_attach"], True)
        finally:
            vm.stop()


if __name__ == "__main__":
    unittest.main()
