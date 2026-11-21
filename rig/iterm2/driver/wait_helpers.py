"""Condition-based polling, JSONL stream readers, F2 fence slicing, and DECRQM parsing."""

from __future__ import annotations

import json
from pathlib import Path
import re
import threading
import time
from typing import Callable, Sequence, TypeVar

from driver.cli_args import DriverRuntimeError

T = TypeVar("T")
DECRQM_REPORT_PATTERN = re.compile(r"\x1b\[\?(\d+);(\d+)\$y")
EXPECTED_PASTE_KEYS: tuple[str, ...] = (
    "char:a",
    "char:l",
    "char:p",
    "char:h",
    "char:a",
    "char:b",
    "char:e",
    "char:t",
    "char:a",
)


def poll_until(
    predicate: Callable[[], T | None],
    timeout_seconds: float,
    description: str,
    poll_interval: float = 0.05,
) -> T:
    """Polls `predicate` until it returns a non-None value or raises on bounded timeout."""
    waiter = threading.Event()
    deadline = time.monotonic() + max(timeout_seconds, 0.05)
    last_error: Exception | None = None
    while True:
        try:
            outcome = predicate()
            if outcome is not None and outcome is not False:
                return outcome
        except DriverRuntimeError:
            raise
        except Exception as exc:  # noqa: BLE001
            last_error = exc
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            detail = f" (last error: {last_error})" if last_error is not None else ""
            raise DriverRuntimeError(
                f"Timed out after {timeout_seconds:.2f}s waiting for {description}{detail}.",
                "Inspect the failure screenshot or increase --timeout-ms if the host is under heavy load.",
            )
        waiter.wait(min(poll_interval, remaining))


def read_complete_jsonl_records(log_path: str | Path) -> list[dict[str, object]]:
    """Reads all newline-terminated JSON objects from `log_path`, ignoring partial tail bytes."""
    target = Path(log_path)
    if not target.is_file():
        return []
    raw_text = target.read_text(encoding="utf-8", errors="replace")
    if not raw_text:
        return []
    lines = raw_text.split("\n")
    complete_lines = lines[:-1]
    records: list[dict[str, object]] = []
    for raw_line in complete_lines:
        stripped = raw_line.strip()
        if not stripped:
            continue
        parsed = json.loads(stripped)
        if isinstance(parsed, dict):
            records.append(parsed)
    return records


def read_all_jsonl_records(log_path: str | Path) -> list[dict[str, object]]:
    """Reads all JSONL records from a completed session log."""
    target = Path(log_path)
    if not target.is_file():
        return []
    records: list[dict[str, object]] = []
    for raw_line in target.read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = raw_line.strip()
        if stripped:
            parsed = json.loads(stripped)
            if isinstance(parsed, dict):
                records.append(parsed)
    return records


def is_fence_key_record(record: dict[str, object]) -> bool:
    """Returns True when `record` is an F2 fence key event."""
    return record.get("kind") == "event" and record.get("event_type") == "key" and record.get("code") == "f2"


def is_fence_press_record(record: dict[str, object]) -> bool:
    """Returns True when `record` is an F2 key-press fence boundary."""
    return is_fence_key_record(record) and record.get("key_kind") == "press"


def extract_fence_slice(
    records: Sequence[dict[str, object]],
    start_count: int,
) -> list[dict[str, object]] | None:
    """Returns event records between `start_count` and the next F2 press, or None if not arrived."""
    for index in range(start_count, len(records)):
        if is_fence_press_record(records[index]):
            return [
                rec
                for rec in records[start_count:index]
                if rec.get("kind") == "event" and not is_fence_key_record(rec)
            ]
    return None


def has_delivered_paste_payload(records_after_start: Sequence[dict[str, object]]) -> bool:
    """Returns True once bracketed `paste` or unbracketed `alpha`+`beta` keys arrive."""
    matched_keys = 0
    for rec in records_after_start:
        if rec.get("kind") != "event":
            continue
        if rec.get("event_type") == "paste":
            return True
        if (
            rec.get("event_type") == "key"
            and rec.get("key_kind") == "press"
            and rec.get("code") == EXPECTED_PASTE_KEYS[matched_keys]
        ):
            matched_keys += 1
            if matched_keys == len(EXPECTED_PASTE_KEYS):
                return True
    return False


def wait_for_probe_ready(
    log_path: Path,
    timeout_seconds: float,
    exit_json_path: Path | None = None,
) -> list[dict[str, object]]:
    """Waits until `<log>.ready` exists and `{"kind":"ready"}` is flushed to `log_path`."""
    ready_sentinel = Path(f"{log_path}.ready")

    def _check_ready() -> list[dict[str, object]] | None:
        if ready_sentinel.is_file():
            records = read_complete_jsonl_records(log_path)
            if any(rec.get("kind") == "ready" for rec in records):
                return records
        if exit_json_path is not None and exit_json_path.is_file() and not ready_sentinel.is_file():
            exit_info = json.loads(exit_json_path.read_text(encoding="utf-8"))
            raise DriverRuntimeError(
                f"Program exited before writing ready sentinel (exitCode={exit_info.get('exitCode')}).",
                "Verify <program> supports --log <path> and writes <path>.ready.",
            )
        return None

    return poll_until(_check_ready, timeout_seconds, f"ready sentinel '{ready_sentinel.name}'")


def parse_decrqm_snapshot(raw_reply: str) -> dict[str, object]:
    """Converts DECRQM replies (`CSI ? <mode> ; <pm> $ y`) into normalized terminal mode state."""
    mode_states: dict[int, int] = {}
    for match in DECRQM_REPORT_PATTERN.finditer(raw_reply):
        mode_states[int(match.group(1))] = int(match.group(2))
    tracking = "none"
    if mode_states.get(1003) == 1:
        tracking = "any"
    elif mode_states.get(1002) == 1:
        tracking = "drag"
    elif mode_states.get(1000) == 1:
        tracking = "normal"
    return {
        "mouseTrackingMode": tracking,
        "sgrMouseMode": mode_states.get(1006) == 1,
        "bracketedPasteMode": mode_states.get(2004) == 1,
        "sendFocusMode": mode_states.get(1004) == 1,
        "rawModeStates": {str(k): v for k, v in sorted(mode_states.items())},
    }
