"""Unit tests for Layer 6 scenario error screenshot capture, matrix cleanup, and CLI dispatch."""

from __future__ import annotations

import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

# Parent directory (`rig/iterm2`) added to `sys.path` for `driver` imports.
DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from driver.cli_args import DriverRuntimeError  # noqa: E402
from driver.iterm_window import GuestDefaultsBackup, GuestItermSession, restore_guest_defaults  # noqa: E402
from driver.main import run_cli  # noqa: E402
from driver.matrix_probes import measure_row_1_wheel_direction, run_iterm2_matrix  # noqa: E402
from driver.scenarios import (  # noqa: E402
    ScenarioRuntimeContext,
    capture_failure_screenshot_safe,
    execute_panic_scenario,
    run_named_scenario,
)
from driver.tart_vm import VncEndpoint  # noqa: E402
from driver.vnc_input import WindowGeometry  # noqa: E402


class FakeVnc:
    def __init__(self) -> None:
        self.screenshots: list[Path] = []

    def capture_screenshot(self, destination: Path) -> Path:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(b"\x89PNG\r\n\x1a\n")
        self.screenshots.append(destination)
        return destination


class FailingCaptureVnc:
    def capture_screenshot(self, destination: Path) -> Path:
        raise DriverRuntimeError(f"cannot capture {destination}", "check vnc")


class FakeContext:
    def __init__(self, artifact_dir: Path) -> None:
        self.vnc = FakeVnc()
        self.artifact_dir = artifact_dir
        self.program = "capability_probe"
        self.tmux_mode = None
        self.timeout_ms = 5000
        self.extra_args: list[str] = []
        self.is_restored = False


class ScenarioAndCleanupTests(unittest.TestCase):
    def test_failure_screenshot_is_captured_when_scenario_raises(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            context = FakeContext(Path(temp_dir))

            def failing_batch(_context: FakeContext) -> dict[str, object]:
                raise DriverRuntimeError("step timed out", "Inspect failure screenshot.")

            with self.assertRaises(DriverRuntimeError) as error_context:
                run_named_scenario("batch", context, scenario_overrides={"batch": failing_batch})  # type: ignore[arg-type]

            self.assertEqual(len(context.vnc.screenshots), 1)
            self.assertTrue(context.vnc.screenshots[0].name.endswith("failure-screenshot.png"))
            self.assertTrue(context.vnc.screenshots[0].is_file())
            self.assertIn("failure-screenshot.png", error_context.exception.recovery_hint)

    def test_execute_panic_scenario_verifies_pre_panic_mouse_across_all_tmux_modes(self) -> None:
        geometry = WindowGeometry(93.0, 168.0, 725.0, 543.0, 100, 30, 7.0, 17.0, 2.0)
        with tempfile.TemporaryDirectory() as temp_dir:
            base = Path(temp_dir)
            fake_session = GuestItermSession(
                "p1", base / "p1.jsonl", base / "p1.meta.json", base / "p1.exit.json", "stock", None
            )
            for tmux_mode in ("stock", "cc-stock"):
                context = ScenarioRuntimeContext(
                    vm=mock.MagicMock(),
                    vnc=mock.MagicMock(),
                    program="capability_probe",
                    guest_program_path="/tmp/probe",
                    tmux_mode=tmux_mode,
                    guest_tmux_stocks={"stock": "/tmp/stock.conf", "full": "/tmp/full.conf"},
                    timeout_ms=500,
                )
                with (
                    mock.patch("driver.scenarios.launch_iterm_session", return_value=fake_session),
                    mock.patch("driver.scenarios.wait_for_probe_ready"),
                    mock.patch("driver.scenarios.resolve_session_geometry", return_value=geometry),
                    mock.patch("driver.scenarios.ensure_iterm_window_focused"),
                    mock.patch("driver.scenarios.run_fenced_vnc_step", return_value=[]),
                ):
                    with self.assertRaises(DriverRuntimeError) as panic_error:
                        execute_panic_scenario(context)
                    self.assertIn("active mouse tracking", panic_error.exception.message)

    def test_capture_failure_screenshot_safe_returns_none_and_cleans_temp_dir(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            self.assertIsNone(capture_failure_screenshot_safe(None, Path(temp_dir)))
            created_dirs: list[Path] = []
            real_mkdtemp = tempfile.mkdtemp

            def tracking_mkdtemp(prefix: str = "") -> str:
                allocated = real_mkdtemp(prefix=prefix, dir=temp_dir)
                created_dirs.append(Path(allocated))
                return allocated

            with mock.patch("driver.scenarios.tempfile.mkdtemp", side_effect=tracking_mkdtemp):
                outcome = capture_failure_screenshot_safe(FailingCaptureVnc(), None)
            self.assertIsNone(outcome)
            self.assertEqual(len(created_dirs), 1)
            self.assertFalse(created_dirs[0].exists())

    def test_matrix_always_restores_guest_defaults_on_error(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            context = FakeContext(Path(temp_dir))
            events: list[str] = []

            def fake_probe(_context: FakeContext) -> dict[str, object]:
                events.append("probe_started")
                raise RuntimeError("boom during matrix probe")

            def fake_restore() -> None:
                context.is_restored = True
                events.append("restored")

            with self.assertRaises(RuntimeError):
                run_iterm2_matrix(context, restore_callback=fake_restore, probe_runner=fake_probe)  # type: ignore[arg-type]

            self.assertTrue(context.is_restored)
            self.assertEqual(events, ["probe_started", "restored"])

    def test_row_1_wheel_sweeps_off_then_on_and_restores_backed_up_scroll_direction(self) -> None:
        scroll_states: list[bool] = []
        geometry = WindowGeometry(93.0, 168.0, 725.0, 543.0, 100, 30, 7.0, 17.0, 2.0)
        with tempfile.TemporaryDirectory() as temp_dir:
            base = Path(temp_dir)
            fake_session = GuestItermSession(
                "w1", base / "w1.jsonl", base / "w1.meta.json", base / "w1.exit.json", None, None
            )
            context = ScenarioRuntimeContext(
                vm=mock.MagicMock(),
                vnc=mock.MagicMock(),
                program="capability_probe",
                guest_program_path="/tmp/probe",
                tmux_mode=None,
                guest_tmux_stocks={},
                timeout_ms=500,
                is_initial_natural_scrolling=False,
            )
            with (
                mock.patch(
                    "driver.matrix_probes.set_guest_natural_scrolling",
                    side_effect=lambda _vm, is_enabled: scroll_states.append(is_enabled),
                ),
                mock.patch("driver.matrix_probes.launch_iterm_session", return_value=fake_session),
                mock.patch("driver.matrix_probes.prepare_ready_interactive_session", return_value=geometry),
                mock.patch(
                    "driver.matrix_probes.run_fenced_vnc_step",
                    return_value=[{"event_type": "mouse", "mouse_kind": "scroll_down"}],
                ),
                mock.patch("driver.matrix_probes.finish_interactive_clean_exit"),
            ):
                result = measure_row_1_wheel_direction(context)
        self.assertIs(result["naturalScrollingOff"]["naturalScrolling"], False)  # type: ignore[index]
        self.assertIs(result["naturalScrollingOn"]["naturalScrolling"], True)  # type: ignore[index]
        self.assertEqual(scroll_states, [False, True, False])

    def test_restore_guest_defaults_deletes_temporary_plist_backup_and_restores_scroll(self) -> None:
        fake_vm = mock.MagicMock()
        backup = GuestDefaultsBackup(
            plist_backup_guest_path="/tmp/tuikit_iterm2_plist_backup.plist",
            had_swipe_direction=True,
            is_natural_swipe_direction=False,
            had_click_show_desktop=False,
            is_click_show_desktop_enabled=True,
        )
        restore_guest_defaults(fake_vm, backup)
        executed_command = fake_vm.exec_guest.call_args[0][0]
        self.assertIn("rm -f /tmp/tuikit_iterm2_plist_backup.plist", executed_command)
        self.assertIn("-bool false", executed_command)

    def test_run_cli_matrix_invokes_guest_defaults_restore_only_once(self) -> None:
        backup = GuestDefaultsBackup(
            plist_backup_guest_path="/tmp/tuikit_iterm2_plist_backup.plist",
            had_swipe_direction=True,
            is_natural_swipe_direction=True,
        )
        fake_vm = mock.MagicMock()
        fake_vm.boot.return_value = VncEndpoint("127.0.0.1", 59000, "pw")
        captured_stdout = io.StringIO()
        captured_stderr = io.StringIO()
        with (
            mock.patch("driver.main.resolve_host_binary", side_effect=lambda name, _env: f"/bin/{name}"),
            mock.patch("driver.main.resolve_or_build_program_binary", return_value=Path("/tmp/probe")),
            mock.patch("driver.main.TartVmSession", return_value=fake_vm),
            mock.patch("driver.main.VncController"),
            mock.patch("driver.main.backup_guest_defaults", return_value=backup),
            mock.patch("driver.main.restore_guest_defaults") as restore_mock,
            mock.patch(
                "driver.main.run_iterm2_matrix",
                side_effect=lambda _ctx, restore_callback: (restore_callback(), {"scenario": "matrix"})[1],
            ),
            mock.patch("sys.stdout", captured_stdout),
            mock.patch("sys.stderr", captured_stderr),
        ):
            exit_code = run_cli(["capability_probe", "matrix"], env={"NO_COLOR": "1"})
        self.assertEqual(exit_code, 0)
        self.assertEqual(restore_mock.call_count, 1)
        fake_vm.stop.assert_called_once()
        report = json.loads(captured_stdout.getvalue())
        self.assertEqual(report["scenario"], "matrix")

    def test_run_cli_help_writes_usage_to_stdout_and_exits_zero(self) -> None:
        captured_stdout = io.StringIO()
        with mock.patch("sys.stdout", captured_stdout):
            exit_code = run_cli(["--help"], env={"NO_COLOR": "1"})
        self.assertEqual(exit_code, 0)
        self.assertIn("Usage:", captured_stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
