"""Unit tests for Layer 6 scenario error screenshot capture, matrix cleanup, and VM teardown."""

from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from unittest import mock

from driver.cli_args import DriverRuntimeError, ParsedCliArgs  # noqa: E402
from driver.iterm_window import GuestDefaultsBackup, restore_guest_defaults  # noqa: E402
from driver.main import _run_booted_vm_payload  # noqa: E402
from driver.matrix_probes import measure_row_1_wheel_direction, run_iterm2_matrix  # noqa: E402
from driver.scenarios import ScenarioRuntimeContext, capture_failure_screenshot_safe, run_named_scenario  # noqa: E402


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

    def test_row_1_wheel_restores_backed_up_scroll_direction(self) -> None:
        scroll_states: list[bool] = []
        fake_context = mock.MagicMock(spec=ScenarioRuntimeContext)
        fake_context.vm = mock.MagicMock()
        fake_context.is_initial_natural_scrolling = False
        with (
            mock.patch(
                "driver.matrix_probes._measure_wheel_under_scroll_setting",
                side_effect=lambda _ctx, is_natural: {"naturalScrolling": is_natural},
            ),
            mock.patch(
                "driver.matrix_probes.set_guest_natural_scrolling",
                side_effect=lambda _vm, is_enabled: scroll_states.append(is_enabled),
            ),
        ):
            result = measure_row_1_wheel_direction(fake_context)
        self.assertEqual(result["naturalScrollingOff"], {"naturalScrolling": False})
        self.assertEqual(scroll_states, [False])

    def test_restore_guest_defaults_removes_backup_plist_and_matrix_restores_once(self) -> None:
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

        parsed = ParsedCliArgs(False, "capability_probe", "matrix", None, 5000, [])
        theme = {"accent": "", "reset": ""}
        with (
            mock.patch("driver.main._stage_guest_binaries_and_configs", return_value=("/tmp/p", {})),
            mock.patch("driver.main.backup_guest_defaults", return_value=backup),
            mock.patch("driver.main.restore_guest_defaults") as restore_mock,
            mock.patch("driver.main.run_iterm2_matrix", side_effect=lambda _ctx, restore_callback: (restore_callback(), {"scenario": "matrix"})[1]),
        ):
            payload = _run_booted_vm_payload(fake_vm, mock.MagicMock(), parsed, Path("/tmp/p"), {}, theme)
        self.assertEqual(payload["scenario"], "matrix")
        self.assertEqual(restore_mock.call_count, 1)


if __name__ == "__main__":
    unittest.main()
