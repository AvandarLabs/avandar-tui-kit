"""Unit tests for pure Layer 6 CLI parsing, VNC math, waiting, and host tool lookup."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest

# Parent directory (`rig/iterm2`) added to `sys.path` for `driver` imports.
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
    wait_for_probe_ready,
)

# Representative Retina iTerm2 window geometry for deterministic coordinate assertions.
SAMPLE_RETINA_GEOMETRY = WindowGeometry(
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


class CliArgsTests(unittest.TestCase):
    def test_help_flags_short_circuit_before_double_dash(self) -> None:
        for flag in ("-h", "--help"):
            parsed = parse_cli_args([flag])
            self.assertTrue(parsed.is_help)
            self.assertIsNone(parsed.program)
        passthrough = parse_cli_args(["capability_probe", "--", "--help"])
        self.assertFalse(passthrough.is_help)
        self.assertEqual(passthrough.extra_args, ["--help"])

    def test_parse_cli_args_defaults_to_all_for_probe_and_passthrough_for_custom_programs(self) -> None:
        default_probe = parse_cli_args(["capability_probe"])
        self.assertEqual(default_probe.scenario, "all")
        self.assertIsNone(default_probe.tmux_mode)
        custom_program = parse_cli_args(["custom_example"])
        self.assertEqual(custom_program.scenario, "passthrough")

    def test_parse_cli_args_accepts_tmux_and_timeout_options_before_or_after_program(self) -> None:
        before_program = parse_cli_args(["--tmux", "stock", "capability_probe", "batch"])
        self.assertEqual(before_program.tmux_mode, "stock")
        self.assertEqual(before_program.scenario, "batch")
        after_program = parse_cli_args(
            ["capability_probe", "all", "--tmux=cc-full", "--timeout-ms", "60000"]
        )
        self.assertEqual(after_program.tmux_mode, "cc-full")
        self.assertEqual(after_program.timeout_ms, 60000)

    def test_parse_cli_args_rejects_invalid_combinations_with_exit_code_two(self) -> None:
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

    def test_create_cli_theme_suppresses_ansi_codes_for_no_color_and_dumb_term(self) -> None:
        plain_theme = create_cli_theme(True, {"NO_COLOR": "1"})
        self.assertEqual(plain_theme["error"], "")
        dumb_theme = create_cli_theme(True, {"TERM": "dumb"})
        self.assertEqual(dumb_theme["heading"], "")
        help_text = build_help_text(plain_theme)
        self.assertIn("Usage:", help_text)
        self.assertNotIn("\x1b[", help_text)


class VncAndWindowMathTests(unittest.TestCase):
    def test_cell_center_to_vnc_pixels_maps_cells_to_retina_pixel_centers(self) -> None:
        self.assertEqual(cell_center_to_vnc_pixels(SAMPLE_RETINA_GEOMETRY, 0, 0), (203, 417))
        self.assertEqual(cell_center_to_vnc_pixels(SAMPLE_RETINA_GEOMETRY, 10, 5), (343, 587))

    def test_normalize_cell_size_pt_scales_retina_dimensions_and_rejects_invalid_bounds(self) -> None:
        self.assertEqual(normalize_cell_size_pt(7.0, 17.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        self.assertEqual(normalize_cell_size_pt(14.0, 34.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        self.assertEqual(normalize_cell_size_pt(16.0, 32.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))
        self.assertEqual(normalize_cell_size_pt(0.0, 17.0, 725.0, 543.0, 100, 30, 2.0), (7.0, 17.0))

    def test_focus_and_titlebar_helpers_target_desktop_margin_and_window_header(self) -> None:
        outside_x, outside_y = outside_focus_point_vnc_pixels(SAMPLE_RETINA_GEOMETRY)
        self.assertLess(outside_x, int(SAMPLE_RETINA_GEOMETRY.window_x_pt * 2))
        self.assertGreaterEqual(outside_y, 80)
        title_x, title_y = titlebar_point_vnc_pixels(SAMPLE_RETINA_GEOMETRY)
        self.assertGreater(title_x, int((SAMPLE_RETINA_GEOMETRY.window_x_pt + 80) * 2))
        self.assertGreater(title_y, int(SAMPLE_RETINA_GEOMETRY.window_y_pt * 2))

    def test_build_drag_tokens_emits_interpolated_moves_without_slow_drag_verb(self) -> None:
        drag_tokens = build_drag_tokens((200, 300), (260, 300), modifiers=("meta",), steps=3)
        self.assertNotIn("drag", drag_tokens)
        self.assertIn("mousedown", drag_tokens)
        self.assertIn("mouseup", drag_tokens)
        self.assertEqual(drag_tokens.count("keydown"), 1)
        self.assertEqual(drag_tokens.count("keyup"), 1)

    def test_click_wheel_and_chord_builders_wrap_modifiers_in_reverse_release_order(self) -> None:
        click_tokens = build_click_tokens(
            343, 587, button=1, modifiers=("shift", "meta"), click_count=2
        )
        self.assertEqual(click_tokens.count("click"), 2)
        self.assertEqual(
            click_tokens,
            [
                "move", "343", "587", "keydown", "shift", "keydown", "meta",
                "click", "1", "click", "1", "keyup", "meta", "keyup", "shift",
            ],
        )
        self.assertEqual(build_wheel_tokens(343, 587, "down"), ["move", "343", "587", "click", "4"])
        self.assertEqual(build_wheel_tokens(343, 587, "up"), ["move", "343", "587", "click", "5"])
        self.assertEqual(
            build_chord_tokens("enter", ("meta",)),
            ["keydown", "meta", "key", "enter", "keyup", "meta"],
        )

    def test_select_calibrated_modifiers_matches_option_and_command_flag_bits(self) -> None:
        mapping = select_calibrated_modifiers({"alt": 0x100108, "meta": 0x80120, "super": 0x100})
        self.assertEqual(mapping.option_key, "meta")
        self.assertEqual(mapping.command_key, "alt")

    def test_window_helpers_detect_popup_menus_and_select_frontmost_terminal_grid(self) -> None:
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
    def test_parse_vnc_connection_url_extracts_loopback_endpoint_or_returns_none(self) -> None:
        parsed = parse_vnc_connection_url("VNC server is running at vnc://:sec-ret@127.0.0.1:55123\n")
        self.assertEqual(parsed, VncEndpoint(host="127.0.0.1", port=55123, password="sec-ret"))
        self.assertIsNone(parse_vnc_connection_url("booting guest..."))

    def test_resolve_host_binary_honors_fallback_dirs_unless_strict_path_is_set(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            fake_binary = Path(temp_dir) / "tart"
            fake_binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            os.chmod(fake_binary, 0o755)
            found = resolve_host_binary("tart", {"PATH": "", "HOME": "/nonexistent"}, [temp_dir])
            self.assertEqual(found, str(fake_binary))
            with self.assertRaises(DriverRuntimeError):
                resolve_host_binary(
                    "tart",
                    {"PATH": "", "HOME": "/nonexistent", "RIG_ITERM2_STRICT_PATH": "1"},
                    [temp_dir],
                )

    def test_read_complete_jsonl_records_and_extract_fence_slice_ignore_partial_tail(self) -> None:
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

    def test_wait_for_probe_ready_returns_flushed_records_once_sentinel_exists(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            log_file = Path(temp_dir) / "ready.jsonl"
            log_file.write_text('{"kind":"ready"}\n', encoding="utf-8")
            Path(f"{log_file}.ready").write_text("", encoding="utf-8")
            ready_records = wait_for_probe_ready(log_file, 0.2)
            self.assertEqual(ready_records, [{"kind": "ready"}])

    def test_parse_decrqm_snapshot_normalizes_reset_terminal_modes(self) -> None:
        snapshot = parse_decrqm_snapshot(
            "\x1b[?1000;2$y\x1b[?1002;2$y\x1b[?1003;2$y\x1b[?1006;2$y\x1b[?2004;2$y\x1b[?1004;2$y"
        )
        self.assertEqual(snapshot["mouseTrackingMode"], "none")
        self.assertFalse(snapshot["sgrMouseMode"])
        self.assertFalse(snapshot["bracketedPasteMode"])
        self.assertFalse(snapshot["sendFocusMode"])

    def test_has_delivered_paste_payload_accepts_bracketed_paste_and_complete_raw_keys(self) -> None:
        bracketed_records = [{"kind": "event", "event_type": "paste", "text": "alpha\nbeta"}]
        self.assertTrue(has_delivered_paste_payload(bracketed_records))
        key_records = [
            {"kind": "event", "event_type": "key", "key_kind": "press", "code": key_code}
            for key_code in EXPECTED_PASTE_KEYS
        ]
        self.assertFalse(has_delivered_paste_payload(key_records[:-1]))
        self.assertTrue(has_delivered_paste_payload(key_records))

    def test_poll_until_times_out_on_none_or_value_error_and_propagates_code_bugs(self) -> None:
        self.assertEqual(poll_until(lambda: "ok", 0.2, "instant"), "ok")
        with self.assertRaises(DriverRuntimeError) as timeout_context:
            poll_until(lambda: None, 0.05, "never ready", poll_interval=0.01)
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


if __name__ == "__main__":
    unittest.main()
