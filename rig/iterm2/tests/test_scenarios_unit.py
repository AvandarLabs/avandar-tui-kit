"""Unit tests for Layer 6 scenario error screenshot capture, matrix cleanup, and VM teardown."""

from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from driver.cli_args import DriverRuntimeError  # noqa: E402
from driver.matrix_probes import run_iterm2_matrix  # noqa: E402
from driver.scenarios import capture_failure_screenshot_safe, run_named_scenario  # noqa: E402


class FakeVnc:
    def __init__(self) -> None:
        self.screenshots: list[Path] = []

    def capture_screenshot(self, destination: Path) -> Path:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(b"\x89PNG\r\n\x1a\n")
        self.screenshots.append(destination)
        return destination


class FakeContext:
    def __init__(self, artifact_dir: Path, should_fail: bool = False) -> None:
        self.vnc = FakeVnc()
        self.artifact_dir = artifact_dir
        self.should_fail = should_fail
        self.program = "capability_probe"
        self.tmux_mode = None
        self.timeout_ms = 5000
        self.extra_args: list[str] = []
        self.restored = False


class ScenarioAndCleanupTests(unittest.TestCase):
    def test_failure_screenshot_is_captured_when_scenario_raises(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ctx = FakeContext(Path(tmp), should_fail=True)

            def failing_batch(_ctx: FakeContext) -> dict[str, object]:
                raise DriverRuntimeError("step timed out", "Inspect failure screenshot.")

            with self.assertRaises(DriverRuntimeError) as err_ctx:
                run_named_scenario("batch", ctx, scenario_overrides={"batch": failing_batch})

            self.assertEqual(len(ctx.vnc.screenshots), 1)
            self.assertTrue(ctx.vnc.screenshots[0].name.endswith("failure-screenshot.png"))
            self.assertTrue(ctx.vnc.screenshots[0].is_file())
            self.assertIn("failure-screenshot.png", err_ctx.exception.recovery_hint)

    def test_capture_failure_screenshot_safe_returns_none_when_vnc_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = capture_failure_screenshot_safe(None, Path(tmp))
            self.assertIsNone(result)

    def test_matrix_always_restores_guest_defaults_on_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            ctx = FakeContext(Path(tmp))
            events: list[str] = []

            def fake_probe(_ctx: FakeContext) -> dict[str, object]:
                events.append("probe_started")
                raise RuntimeError("boom during matrix probe")

            def fake_restore() -> None:
                ctx.restored = True
                events.append("restored")

            with self.assertRaises(RuntimeError):
                run_iterm2_matrix(ctx, restore_callback=fake_restore, probe_runner=fake_probe)

            self.assertTrue(ctx.restored)
            self.assertEqual(events, ["probe_started", "restored"])


if __name__ == "__main__":
    unittest.main()
