"""Unit tests for pure Layer 6 CLI parsing, VNC math, waiting, and host tool lookup."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest

DRIVER_PARENT = Path(__file__).resolve().parents[1]
if str(DRIVER_PARENT) not in sys.path:
    sys.path.insert(0, str(DRIVER_PARENT))

from driver.cli_args import (  # noqa: E402
    CliUsageError,
    DriverRuntimeError,
    build_help_text,
    create_cli_theme,
    merge_probe_args,
    parse_cli_args,
)
from driver.iterm_window import (  # noqa: E402
    has_popup_menu_window,
    select_active_iterm_window,
)
from driver.tart_vm import (  # noqa: E402
    VncEndpoint,
    parse_vnc_connection_url,
    resolve_host_binary,
)
from driver.vnc_input import (  # noqa: E402
    WindowGeometry,
    build_chord_tokens,
    build_click_tokens,
    build_drag_tokens,
    build_wheel_tokens,
    cell_center_to_vnc_pixels,
    normalize_cell_size_pt,
    outside_focus_point_vnc_pixels,
    select_calibrated_modifiers,
    titlebar_point_vnc_pixels,
)
from driver.wait_helpers import (  # noqa: E402
    EXPECTED_PASTE_KEYS,
    extract_fence_slice,
    has_delivered_paste_payload,
    parse_decrqm_snapshot,
    poll_until,
    read_complete_jsonl_records,
)


class CliArgsTests(unittest.TestCase):
    def test_help_flags_short_circuit_before_double_dash(self) -> None:
        for flag in ("-h", "--help"):
            parsed = parse_cli_args([flag])
            self.assertTrue(parsed.is_help)
            self.assertIsNone(parsed.program)
        passthrough = parse_cli_args(["capability_probe", "--", "--help"])
        self.assertFalse(passthrough.is_help)
        self.assertEqual(passthrough.extra_args, ["--help"])

    def test_defaults_and_tmux_before_or_after_program(self) -> None:
        default_probe = parse_cli_args(["capability_probe"])
        self.assertEqual(default_probe.scenario, "all")
        self.assertIsNone(default_probe.tmux_mode)

        custom_program = parse_cli_args(["custom_example"])
        self.assertEqual(custom_program.scenario, "passthrough")

        before_program = parse_cli_args(["--tmux", "stock", "capability_probe", "batch"])
        self.assertEqual(before_program.tmux_mode, "stock")
        self.assertEqual(before_program.scenario, "batch")

        after_program = parse_cli_args(["capability_probe", "all", "--tmux=cc-full", "--timeout-ms", "60000"])
        self.assertEqual(after_program.tmux_mode, "cc-full")
        self.assertEqual(after_program.timeout_ms, 60000)

    def test_rejects_invalid_cli_combinations(self) -> None:
        bad_inputs = [
            [],
            ["--unknown"],
            ["bad/program"],
            ["capability_probe", "not_a_scenario"],
            ["capability_probe", "--tmux", "invalid"],
            ["--tmux", "stock", "--tmux", "full", "capability_probe"],
            ["--tmux", "stock", "capability_probe", "matrix"],
            ["capability_probe", "--timeout-ms", "0"],
            ["capability_probe", "--osc52"],
            ["capability_probe", "--osc52-form"],
            ["capability_probe", "--osc52", "--", "--extra"],
            ["capability_probe", "--osc52-form", "--", "--extra"],
            ["capability_probe", "--log", "/tmp/x.jsonl"],
            ["capability_probe", "--", "--log=/tmp/x.jsonl"],
        ]
        for argv in bad_inputs:
            with self.subTest(argv=argv):
                with self.assertRaises(CliUsageError) as error_context:
                    parse_cli_args(argv)
                self.assertEqual(error_context.exception.exit_code, 2)
                self.assertTrue(error_context.exception.recovery_hint)

    def test_merge_probe_args_gives_managed_flags_precedence(self) -> None:
        merged = merge_probe_args(
            ["--log", "/managed.jsonl", "--batch"],
            ["--batch", "--osc52", "token_one", "--custom-flag"],
        )
        self.assertEqual(
            merged,
            ["--log", "/managed.jsonl", "--batch", "--osc52", "token_one", "--custom-flag"],
        )

    def test_theme_honors_no_color_and_dumb_term(self) -> None:
        plain_theme = create_cli_theme(True, {"NO_COLOR": "1"})
        self.assertEqual(plain_theme["error"], "")
        dumb_theme = create_cli_theme(True, {"TERM": "dumb"})
        self.assertEqual(dumb_theme["heading"], "")
        help_text = build_help_text(plain_theme)
        self.assertIn("Usage:", help_text)
        self.assertNotIn("\x1b[", help_text)


class VncAndWindowMathTests(unittest.TestCase):
    def test_cell_center_to_vnc_pixels_matches_retina_geometry(self) -> None:
        geometry = WindowGeometry(
            window_x_pt=93.0,
            window_y_pt=168.0,
            window_width_pt=725.0,
            window_height_pt=543.0,
            cols=100,
            rows=30,
            cell_width_pt=7.0,
            cell_height_pt=17.0,
            backing_scale=2.0,
        )
        self.assertEqual(cell_center_to_vnc_pixels(geometry, 0, 0), (203, 417))
        self.assertEqual(cell_center_to_vnc_pixels(geometry, 10, 5), (343, 587))
        self.assertEqual(normalize_cell_size_pt(7.0, 17.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        self.assertEqual(normalize_cell_size_pt(14.0, 34.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        self.assertEqual(normalize_cell_size_pt(16.0, 32.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        outside_x, outside_y = outside_focus_point_vnc_pixels(geometry)
        self.assertLess(outside_x, int(geometry.window_x_pt * 2))
        self.assertGreaterEqual(outside_y, 80)
        title_x, title_y = titlebar_point_vnc_pixels(geometry)
        self.assertGreater(title_x, int((geometry.window_x_pt + 80) * 2))
        self.assertGreater(title_y, int(geometry.window_y_pt * 2))

    def test_vnc_token_builders_never_emit_slow_drag_command(self) -> None:
        drag_tokens = build_drag_tokens((200, 300), (260, 300), modifiers=("meta",), steps=3)
        self.assertNotIn("drag", drag_tokens)
        self.assertIn("mousedown", drag_tokens)
        self.assertIn("mouseup", drag_tokens)
        self.assertEqual(drag_tokens.count("keydown"), 1)
        self.assertEqual(drag_tokens.count("keyup"), 1)

        click_tokens = build_click_tokens(343, 587, button=1, modifiers=("shift",), click_count=2)
        self.assertEqual(click_tokens.count("click"), 2)
        self.assertEqual(build_wheel_tokens(343, 587, "down"), ["move", "343", "587", "click", "4"])
        self.assertEqual(build_wheel_tokens(343, 587, "up"), ["move", "343", "587", "click", "5"])
        self.assertEqual(
            build_chord_tokens("enter", ("meta",)),
            ["keydown", "meta", "key", "enter", "keyup", "meta"],
        )

    def test_modifier_calibration_detects_option_and_command_bits(self) -> None:
        mapping = select_calibrated_modifiers({"alt": 0x100108, "meta": 0x80120, "super": 0x100})
        self.assertEqual(mapping.option_key, "meta")
        self.assertEqual(mapping.command_key, "alt")

    def test_window_selection_and_popup_menu_detection(self) -> None:
        windows = [
            {"kCGWindowLayer": 101, "kCGWindowBounds": {"X": 200, "Y": 300, "Width": 250, "Height": 380}},
            {"kCGWindowLayer": 0, "kCGWindowBounds": {"X": 10, "Y": 10, "Width": 120, "Height": 28}},
            {"kCGWindowLayer": 0, "kCGWindowBounds": {"X": 93, "Y": 168, "Width": 725, "Height": 543}},
        ]
        self.assertTrue(has_popup_menu_window(windows))
        selected = select_active_iterm_window(windows)
        self.assertEqual(selected["kCGWindowLayer"], 0)
        self.assertEqual(selected["kCGWindowBounds"]["Height"], 543)


class WaitAndLookupTests(unittest.TestCase):
    def test_parse_vnc_url_and_resolve_host_binary(self) -> None:
        parsed = parse_vnc_connection_url("VNC server is running at vnc://:sec-ret@127.0.0.1:55123\n")
        self.assertEqual(parsed, VncEndpoint(host="127.0.0.1", port=55123, password="sec-ret"))
        self.assertIsNone(parse_vnc_connection_url("booting guest..."))

        with tempfile.TemporaryDirectory() as temp_dir:
            fake_bin = Path(temp_dir) / "tart"
            fake_bin.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            os.chmod(fake_bin, 0o755)
            found = resolve_host_binary("tart", {"PATH": "", "HOME": "/nonexistent"}, [temp_dir])
            self.assertEqual(found, str(fake_bin))
            with self.assertRaises(DriverRuntimeError):
                resolve_host_binary(
                    "tart",
                    {"PATH": "", "HOME": "/nonexistent", "RIG_ITERM2_STRICT_PATH": "1"},
                    [temp_dir],
                )

    def test_fence_slice_complete_jsonl_and_decrqm_parser(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            log_file = Path(temp_dir) / "probe.jsonl"
            log_file.write_text(
                '{"kind":"ready"}\n'
                '{"kind":"event","event_type":"key","code":"up","key_kind":"press"}\n'
                '{"kind":"event","event_type":"key","code":"f2","key_kind":"press"}\n'
                '{"kind":"partial',
                encoding="utf-8",
            )
            records = read_complete_jsonl_records(log_file)
            self.assertEqual(len(records), 3)
            sliced = extract_fence_slice(records, 1)
            assert sliced is not None
            self.assertEqual(len(sliced), 1)
            self.assertEqual(sliced[0]["code"], "up")

        snapshot = parse_decrqm_snapshot(
            "\x1b[?1000;2$y\x1b[?1002;2$y\x1b[?1003;2$y\x1b[?1006;2$y\x1b[?2004;2$y\x1b[?1004;2$y"
        )
        self.assertEqual(snapshot["mouseTrackingMode"], "none")
        self.assertFalse(snapshot["sgrMouseMode"])
        self.assertFalse(snapshot["bracketedPasteMode"])
        self.assertFalse(snapshot["sendFocusMode"])
        self.assertEqual(poll_until(lambda: "ok", 0.2, "instant"), "ok")

    def test_has_delivered_paste_payload_bracketed_and_unbracketed(self) -> None:
        bracketed_records = [{"kind": "event", "event_type": "paste", "text": "alpha\nbeta"}]
        self.assertTrue(has_delivered_paste_payload(bracketed_records))

        key_records = [
            {"kind": "event", "event_type": "key", "key_kind": "press", "code": key_code}
            for key_code in EXPECTED_PASTE_KEYS
        ]
        self.assertFalse(has_delivered_paste_payload(key_records[:-1]))
        self.assertTrue(has_delivered_paste_payload(key_records))

    def test_poll_until_timeout_and_programming_error_propagation(self) -> None:
        def always_none() -> None:
            return None

        with self.assertRaises(DriverRuntimeError) as timeout_context:
            poll_until(always_none, 0.05, "never ready", poll_interval=0.01)
        self.assertIn("never ready", timeout_context.exception.message)

        def transient_value_error() -> None:
            raise ValueError("partial json")

        with self.assertRaises(DriverRuntimeError) as value_context:
            poll_until(transient_value_error, 0.05, "json flush", poll_interval=0.01)
        self.assertIn("partial json", value_context.exception.message)

        call_count = 0

        def buggy_predicate() -> str:
            nonlocal call_count
            call_count += 1
            missing_map: dict[str, str] = {}
            return missing_map["nonexistent"]

        with self.assertRaises(KeyError):
            poll_until(buggy_predicate, 1.0, "should fail fast", poll_interval=0.2)
        self.assertEqual(call_count, 1)

        with self.assertRaises(TypeError):
            poll_until(lambda: 1 + "bad", 1.0, "type bug", poll_interval=0.2)  # type: ignore[operator,return-value]


if __name__ == "__main__":
    unittest.main()
