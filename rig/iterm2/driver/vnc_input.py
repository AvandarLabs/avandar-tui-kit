"""Pure cell-to-VNC-pixel geometry, VNC token builders, and modifier calibration."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
import subprocess

from driver.cli_args import DriverRuntimeError
from driver.tart_vm import GUEST_SHARED_ROOT, TartVmSession, VncEndpoint

CG_FLAG_OPTION = 0x80000
CG_FLAG_COMMAND = 0x100000
CANDIDATE_VNC_MODIFIERS: tuple[str, ...] = ("meta", "alt", "super")

CG_FLAGS_SCRIPT = """import ctypes
core_graphics = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
core_graphics.CGEventSourceFlagsState.argtypes = [ctypes.c_int32]
core_graphics.CGEventSourceFlagsState.restype = ctypes.c_uint64
print(hex(core_graphics.CGEventSourceFlagsState(1)))
"""


@dataclass(frozen=True)
class WindowGeometry:
    """Logical CoreGraphics window bounds paired with terminal grid dimensions and Retina scale."""

    window_x_pt: float
    window_y_pt: float
    window_width_pt: float
    window_height_pt: float
    cols: int
    rows: int
    cell_width_pt: float
    cell_height_pt: float
    backing_scale: float = 2.0


@dataclass(frozen=True)
class ModifierMap:
    """Calibrated VNC key names corresponding to macOS Option, Command, Shift, and Control."""

    option_key: str
    command_key: str
    shift_key: str = "shift"
    ctrl_key: str = "ctrl"


def normalize_cell_size_pt(
    raw_width: float,
    raw_height: float,
    window_width_pt: float,
    window_height_pt: float,
    cols: int,
    rows: int,
    backing_scale: float = 2.0,
) -> tuple[float, float]:
    """Normalizes `CSI 16 t` cell dimensions to logical points when `tmux` reports Retina pixels."""
    if raw_width <= 0.0 or raw_height <= 0.0:
        return (7.0, 17.0)
    candidate_width, candidate_height = raw_width, raw_height
    if (cols * candidate_width > window_width_pt or rows * candidate_height > window_height_pt) and backing_scale > 1.0:
        candidate_width = raw_width / backing_scale
        candidate_height = raw_height / backing_scale
    if cols * candidate_width > window_width_pt or rows * candidate_height > window_height_pt:
        return (7.0, 17.0)
    return (candidate_width, candidate_height)


def cell_center_to_vnc_pixels(geometry: WindowGeometry, col: int, row: int) -> tuple[int, int]:
    """Maps 0-indexed `(col, row)` cell coordinates to Retina VNC framebuffer pixel coordinates."""
    origin_x_pt = geometry.window_x_pt + 5.0
    origin_y_pt = geometry.window_y_pt + (
        geometry.window_height_pt - (geometry.rows * geometry.cell_height_pt) - 1.0
    )
    center_x_pt = origin_x_pt + (col + 0.5) * geometry.cell_width_pt
    center_y_pt = origin_y_pt + (row + 0.5) * geometry.cell_height_pt
    return (
        int(round(center_x_pt * geometry.backing_scale)),
        int(round(center_y_pt * geometry.backing_scale)),
    )


def outside_focus_point_vnc_pixels(geometry: WindowGeometry) -> tuple[int, int]:
    """Selects a Finder desktop coordinate guaranteed outside the iTerm2 window and menu bar."""
    left_margin_px = int(geometry.window_x_pt * geometry.backing_scale)
    if left_margin_px >= 80:
        return (30, 200)
    right_edge_px = int((geometry.window_x_pt + geometry.window_width_pt) * geometry.backing_scale)
    return (right_edge_px + 40, 200)


def titlebar_point_vnc_pixels(geometry: WindowGeometry) -> tuple[int, int]:
    """Selects the center of the iTerm2 title bar (away from window buttons) to restore focus."""
    center_x_pt = geometry.window_x_pt + (geometry.window_width_pt * 0.5)
    title_y_pt = geometry.window_y_pt + 14.0
    return (
        int(round(center_x_pt * geometry.backing_scale)),
        int(round(title_y_pt * geometry.backing_scale)),
    )


def _modifier_down_tokens(modifiers: Sequence[str]) -> list[str]:
    tokens: list[str] = []
    for modifier_name in modifiers:
        tokens.extend(["keydown", modifier_name])
    return tokens


def _modifier_up_tokens(modifiers: Sequence[str]) -> list[str]:
    tokens: list[str] = []
    for modifier_name in reversed(modifiers):
        tokens.extend(["keyup", modifier_name])
    return tokens


def build_move_tokens(points: Sequence[tuple[int, int]]) -> list[str]:
    """Builds fast instantaneous VNC pointer move tokens across `points`."""
    tokens: list[str] = []
    for x_px, y_px in points:
        tokens.extend(["move", str(x_px), str(y_px)])
    return tokens


def build_click_tokens(
    x_px: int,
    y_px: int,
    button: int = 1,
    modifiers: Sequence[str] = (),
    click_count: int = 1,
) -> list[str]:
    """Builds VNC pointer move + optional modifiers + click token sequence."""
    clicks: list[str] = []
    for _ in range(max(click_count, 1)):
        clicks.extend(["click", str(button)])
    return [
        "move",
        str(x_px),
        str(y_px),
        *_modifier_down_tokens(modifiers),
        *clicks,
        *_modifier_up_tokens(modifiers),
    ]


def build_drag_tokens(
    start_xy: tuple[int, int],
    end_xy: tuple[int, int],
    button: int = 1,
    modifiers: Sequence[str] = (),
    steps: int = 3,
) -> list[str]:
    """Builds multi-point `mousedown`/`move`/`mouseup` tokens without `vncdo`'s slow `drag`."""
    segment_count = max(steps, 2)
    waypoints: list[tuple[int, int]] = []
    for index in range(1, segment_count + 1):
        ratio = index / segment_count
        way_x = int(round(start_xy[0] + (end_xy[0] - start_xy[0]) * ratio))
        way_y = int(round(start_xy[1] + (end_xy[1] - start_xy[1]) * ratio))
        waypoints.append((way_x, way_y))
    return [
        "move",
        str(start_xy[0]),
        str(start_xy[1]),
        *_modifier_down_tokens(modifiers),
        "mousedown",
        str(button),
        *build_move_tokens(waypoints),
        "mouseup",
        str(button),
        *_modifier_up_tokens(modifiers),
    ]


def build_wheel_tokens(x_px: int, y_px: int, direction: str) -> list[str]:
    """Builds VNC wheel button tokens (`4` vs `5`) at `(x_px, y_px)`."""
    if direction not in ("down", "up"):
        raise ValueError(f"Unsupported wheel direction '{direction}'")
    button = "4" if direction == "down" else "5"
    return ["move", str(x_px), str(y_px), "click", button]


def build_chord_tokens(key_name: str, modifiers: Sequence[str] = ()) -> list[str]:
    """Builds VNC key chord tokens wrapping `key <key_name>` with `keydown`/`keyup`."""
    if not modifiers:
        return ["key", key_name]
    return [*_modifier_down_tokens(modifiers), "key", key_name, *_modifier_up_tokens(modifiers)]


def select_calibrated_modifiers(flag_samples: Mapping[str, int]) -> ModifierMap:
    """Selects VNC modifier names from sampled `CGEventSourceFlagsState` bitmasks."""
    option_key: str | None = None
    command_key: str | None = None
    for vnc_modifier, flag_mask in flag_samples.items():
        if option_key is None and (flag_mask & CG_FLAG_OPTION):
            option_key = vnc_modifier
        if command_key is None and (flag_mask & CG_FLAG_COMMAND):
            command_key = vnc_modifier
    return ModifierMap(
        option_key=option_key or "meta",
        command_key=command_key or "alt",
    )


class VncController:
    """Executes `vncdo` commands against the booted VM's loopback VNC server."""

    def __init__(
        self,
        vncdo_bin: str,
        endpoint: VncEndpoint,
        env: Mapping[str, str] | None = None,
    ) -> None:
        self.vncdo_bin = vncdo_bin
        self.endpoint = endpoint
        self.env = None if env is None else dict(env)
        self._cached_modifiers: ModifierMap | None = None

    def run_tokens(self, tokens: Sequence[str], timeout_seconds: float = 15.0) -> None:
        """Invokes `vncdo` synchronously with the given command tokens."""
        server = f"{self.endpoint.host}::{self.endpoint.port}"
        command = [self.vncdo_bin, "-s", server, "-p", self.endpoint.password, *tokens]
        try:
            completed = subprocess.run(
                command,
                capture_output=True,
                text=True,
                env=self.env,
                timeout=timeout_seconds,
                check=False,
            )
        except subprocess.TimeoutExpired as timeout_error:
            raise DriverRuntimeError(
                f"vncdo command {list(tokens[:8])} timed out after {timeout_seconds:.1f}s.",
                "Verify the Tart loopback VNC server is responsive or increase --timeout-ms.",
            ) from timeout_error
        except OSError as os_error:
            raise DriverRuntimeError(
                f"Failed to execute vncdo binary '{self.vncdo_bin}': {os_error}",
                "Verify vncdotool is installed and executable.",
            ) from os_error
        if completed.returncode != 0:
            detail = (completed.stderr or completed.stdout).strip()
            raise DriverRuntimeError(
                f"vncdo command {list(tokens[:8])} failed (exit {completed.returncode}): {detail}",
                "Verify vncdotool can reach the Tart loopback VNC server.",
            )

    def send_fence_key(self) -> None:
        """Sends the FIFO `F2` fence key over VNC."""
        self.run_tokens(["key", "f2"])

    def capture_screenshot(self, destination: Path) -> Path:
        """Captures a PNG screenshot of the VNC framebuffer to `destination`."""
        destination.parent.mkdir(parents=True, exist_ok=True)
        self.run_tokens(["capture", str(destination)], timeout_seconds=15.0)
        return destination

    def calibrate_modifiers(self, vm: TartVmSession) -> ModifierMap:
        """Measures which VNC modifier keys arrive in macOS as `Option` vs `Command`."""
        if self._cached_modifiers is not None:
            return self._cached_modifiers
        helper_file = vm.shared_dir / "_cg_flags.py"
        helper_file.write_text(CG_FLAGS_SCRIPT, encoding="utf-8")
        samples: dict[str, int] = {}
        for modifier_name in CANDIDATE_VNC_MODIFIERS:
            self.run_tokens(["keydown", modifier_name])
            try:
                flags_output = vm.exec_guest(
                    f'python3 "{GUEST_SHARED_ROOT}/_cg_flags.py"', 10.0
                ).stdout.strip()
                samples[modifier_name] = int(flags_output, 16) if flags_output.startswith("0x") else 0
            finally:
                self.run_tokens(["keyup", modifier_name])
        self._cached_modifiers = select_calibrated_modifiers(samples)
        return self._cached_modifiers
