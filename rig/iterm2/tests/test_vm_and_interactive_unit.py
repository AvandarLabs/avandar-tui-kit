"""Unit tests for TartVmSession teardown safety, subprocess error wrapping, DECRQM, and pointer assertions."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from driver.cli_args import DriverRuntimeError  # noqa: E402
from driver.interactive_steps import (  # noqa: E402
    assert_iterm2_pointer_results,
    wait_for_session_exit_and_teardown,
)
from driver.iterm_window import GuestItermSession  # noqa: E402
from driver.tart_vm import (  # noqa: E402
    TartVmSession,
    VncEndpoint,
    ensure_vm_present_and_cold,
)
from driver.vnc_input import VncController  # noqa: E402


class StubHungProcess:
    """Simulates a stubborn `tart run` child process that times out on wait."""

    def __init__(self) -> None:
        self.was_terminated = False
        self.was_killed = False
        self.wait_calls: list[float | None] = []

    def poll(self) -> int | None:
        return None

    def terminate(self) -> None:
        self.was_terminated = True

    def kill(self) -> None:
        self.was_killed = True

    def wait(self, timeout: float | None = None) -> int:
        self.wait_calls.append(timeout)
        raise subprocess.TimeoutExpired(cmd=["tart", "run"], timeout=timeout or 1.0)


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
    def test_stop_kills_proc_and_removes_shared_dir_when_tart_stop_times_out(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        shared_dir = vm.shared_dir
        (shared_dir / "payload.txt").write_text("staged", encoding="utf-8")
        hung_proc = StubHungProcess()
        vm._proc = hung_proc  # type: ignore[assignment]
        vm._has_started_vm = True

        with mock.patch(
            "driver.tart_vm.subprocess.run",
            side_effect=subprocess.TimeoutExpired(cmd=["tart", "stop"], timeout=30.0),
        ):
            vm.stop()

        self.assertTrue(hung_proc.was_terminated)
        self.assertTrue(hung_proc.was_killed)
        self.assertFalse(shared_dir.exists())

    def test_stop_skips_tart_stop_when_boot_never_started_vm(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        shared_dir = vm.shared_dir
        self.assertTrue(shared_dir.is_dir())

        with mock.patch("driver.tart_vm.subprocess.run") as run_mock:
            vm.stop()

        run_mock.assert_not_called()
        self.assertFalse(shared_dir.exists())

    def test_boot_reaps_previous_proc_before_retrying_spawn(self) -> None:
        vm = TartVmSession("/usr/local/bin/tart", vm_name="tuikit-macos", env={"HOME": "/nonexistent"})
        first_proc = StubHungProcess()
        expected_endpoint = VncEndpoint(host="127.0.0.1", port=59001, password="pw")
        spawn_count = 0

        def fake_spawn() -> None:
            nonlocal spawn_count
            spawn_count += 1
            if spawn_count == 1:
                vm._proc = first_proc  # type: ignore[assignment]
            else:
                self.assertIsNone(vm._proc)

        with (
            mock.patch("driver.tart_vm.ensure_vm_present_and_cold"),
            mock.patch("driver.tart_vm.wait_until_vm_fully_stopped"),
            mock.patch.object(vm, "_spawn_tart_run", side_effect=fake_spawn),
            mock.patch.object(
                vm,
                "_wait_until_boot_ready",
                side_effect=[DriverRuntimeError("VZErrorDomain transient", "retry"), expected_endpoint],
            ),
        ):
            endpoint = vm.boot(timeout_seconds=10.0)
        self.assertEqual(endpoint, expected_endpoint)
        self.assertTrue(first_proc.was_terminated)
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
        assert_iterm2_pointer_results(empty_cc_steps, "cc-full")


if __name__ == "__main__":
    unittest.main()
