"""CLI argument parsing, semantic terminal styling, and probe flag merging for Layer 6."""

from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Mapping, Sequence

VALID_TMUX_MODES: tuple[str, ...] = ("stock", "full", "cc-stock", "cc-full")
VALID_SCENARIOS: tuple[str, ...] = (
    "all",
    "batch",
    "osc52",
    "interactive",
    "panic",
    "matrix",
    "passthrough",
)
DEFAULT_TIMEOUT_MS = 45000
BOOLEAN_PROBE_FLAGS = frozenset({"--batch", "--push-kitty"})
VALUE_PROBE_FLAGS = frozenset({"--log", "--osc52", "--osc52-form"})
VALID_PROGRAM_PATTERN = re.compile(r"^[A-Za-z0-9_-]+$")
POSITIVE_INT_PATTERN = re.compile(r"^[1-9][0-9]*$")


class DriverError(Exception):
    """Base Layer 6 error carrying an exit status and an actionable recovery hint."""

    def __init__(self, message: str, recovery_hint: str, exit_code: int = 1) -> None:
        super().__init__(message)
        self.message = message
        self.recovery_hint = recovery_hint
        self.exit_code = exit_code


class CliUsageError(DriverError):
    """Usage error exiting with code 2 and recovery guidance."""

    def __init__(self, message: str, recovery_hint: str) -> None:
        super().__init__(message, recovery_hint, exit_code=2)


class DriverRuntimeError(DriverError):
    """Operational failure exiting with code 1 and recovery guidance."""

    def __init__(self, message: str, recovery_hint: str) -> None:
        super().__init__(message, recovery_hint, exit_code=1)


@dataclass(frozen=True)
class ParsedCliArgs:
    """Validated CLI invocation parameters for Layer 6."""

    is_help: bool
    program: str | None
    scenario: str | None
    tmux_mode: str | None
    timeout_ms: int
    extra_args: list[str]


def create_cli_theme(is_tty: bool, env: Mapping[str, str]) -> dict[str, str]:
    """Builds ANSI color tokens while honoring NO_COLOR, TERM=dumb, and non-TTY streams."""
    if not is_tty or bool(env.get("NO_COLOR")) or env.get("TERM") == "dumb":
        return {
            key: ""
            for key in ("reset", "bold", "heading", "accent", "muted", "success", "warn", "error")
        }
    return {
        "reset": "\x1b[0m",
        "bold": "\x1b[1m",
        "heading": "\x1b[1;36m",
        "accent": "\x1b[36m",
        "muted": "\x1b[2m",
        "success": "\x1b[32m",
        "warn": "\x1b[33m",
        "error": "\x1b[31m",
    }


def _help_scenarios_and_options(a: str, h: str, m: str, r: str) -> list[str]:
    return [
        f"{h}Scenarios:{r}",
        f"  {a}all{r}                    Run batch, osc52, interactive, and panic in one cold-booted VM",
        f"  {a}batch{r}                  Run non-interactive capability_probe sections and verify teardown",
        f"  {a}osc52{r}                  Verify OSC 52 clipboard and tmux buffer behavior",
        f"  {a}interactive{r}            Drive VNC pointer, keyboard, paste, focus, and Kitty Enter checks",
        f"  {a}panic{r}                  Trigger F12 panic and verify terminal mode teardown",
        f"  {a}matrix{r}                 Sweep M0 rows 1, 4, 5, 6, 7, 9, 10, 11, 12, 13 and restore guest defaults",
        f"  {a}passthrough{r}            Run a custom <program> with --log managed by the driver",
        "",
        f"{h}Options:{r}",
        f"  {a}-h, --help{r}             Show this help text on stdout and exit 0",
        f"  {a}--tmux <mode>{r}          Run inside private tmux: stock, full, cc-stock, or cc-full",
        f"  {a}--timeout-ms <ms>{r}      Bounded per-phase condition timeout in milliseconds (default: 45000)",
        f"  {a}--{r}                     Forward all remaining arguments verbatim to <program>",
        f"  {m}(Note: --log <path> is managed automatically by the driver and must not be passed manually.){r}",
    ]


def build_help_text(theme: Mapping[str, str]) -> str:
    """Renders structured CLI help text for stdout."""
    b, h, a, m, r = theme["bold"], theme["heading"], theme["accent"], theme["muted"], theme["reset"]
    return "\n".join(
        [
            f"{h}Usage:{r}",
            f"  {b}just iterm2{r} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]",
            f"  {b}./rig/iterm2/run.sh{r} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]",
            "",
            f"{h}Purpose:{r}",
            "  Boot the headless tuikit-macos Tart VM over loopback VNC, drive iTerm2 (alone, in tmux,",
            "  or in tmux -CC), and emit a single machine-parseable JSON report on stdout (Layer 6).",
            "",
            f"{h}Arguments:{r}",
            f"  {a}<program>{r}              Cargo example binary name (for example: capability_probe)",
            f"  {a}[scenario]{r}             Scenario name (default: 'all' for capability_probe, 'passthrough' otherwise)",
            "",
            *_help_scenarios_and_options(a, h, m, r),
            "",
            f"{h}Streams:{r}",
            "  stdout                 Single machine-parseable JSON report object",
            "  stderr                 VM/scenario progress, warnings, errors, and final status summary",
            "",
            f"{h}Examples:{r}",
            "  just iterm2 capability_probe",
            "  just iterm2 capability_probe matrix",
            "  just iterm2 --tmux full capability_probe all",
            "  ./rig/iterm2/run.sh capability_probe all --tmux cc-full",
        ]
    )


def _parse_positive_timeout(raw_value: str | None) -> int:
    if raw_value is None or not POSITIVE_INT_PATTERN.match(raw_value):
        raise CliUsageError(
            f"Option '--timeout-ms' requires a positive integer in milliseconds, got '{raw_value or ''}'.",
            "Pass a positive integer, for example: --timeout-ms 45000",
        )
    return int(raw_value)


def _parse_tmux_mode(existing_mode: str | None, raw_mode: str | None) -> str:
    if existing_mode is not None:
        raise CliUsageError(
            "Duplicate '--tmux' option.",
            "Pass '--tmux' at most once with one of: stock, full, cc-stock, cc-full.",
        )
    if raw_mode not in VALID_TMUX_MODES:
        raise CliUsageError(
            f"Option '--tmux' requires one of {', '.join(VALID_TMUX_MODES)}, got '{raw_mode or ''}'.",
            "Pass '--tmux stock', '--tmux full', '--tmux cc-stock', or '--tmux cc-full'.",
        )
    return raw_mode


def _reject_manual_log_flag(tokens: Sequence[str]) -> None:
    for token in tokens:
        if token == "--log" or token.startswith("--log="):
            raise CliUsageError(
                "Do not pass '--log' manually; the Layer 6 driver manages JSONL log paths automatically.",
                "Omit '--log' and read the structured JSON report from stdout instead.",
            )


def _consume_option_token(
    tokens: Sequence[str],
    idx: int,
    has_program: bool,
    tmux_mode: str | None,
    timeout_ms: int,
    extra_args: list[str],
) -> tuple[int, str | None, int]:
    token = tokens[idx]
    if token == "--tmux":
        next_val = tokens[idx + 1] if idx + 1 < len(tokens) and tokens[idx + 1] != "--" else None
        return idx + 2, _parse_tmux_mode(tmux_mode, next_val), timeout_ms
    if token.startswith("--tmux="):
        return idx + 1, _parse_tmux_mode(tmux_mode, token.split("=", 1)[1]), timeout_ms
    if token == "--timeout-ms":
        next_val = tokens[idx + 1] if idx + 1 < len(tokens) and tokens[idx + 1] != "--" else None
        return idx + 2, tmux_mode, _parse_positive_timeout(next_val)
    if token.startswith("--timeout-ms="):
        return idx + 1, tmux_mode, _parse_positive_timeout(token.split("=", 1)[1])
    if has_program and token in BOOLEAN_PROBE_FLAGS:
        extra_args.append(token)
        return idx + 1, tmux_mode, timeout_ms
    if has_program and token in ("--osc52", "--osc52-form") and idx + 1 < len(tokens):
        extra_args.extend([token, tokens[idx + 1]])
        return idx + 2, tmux_mode, timeout_ms
    if has_program and (token.startswith("--osc52=") or token.startswith("--osc52-form=")):
        extra_args.append(token)
        return idx + 1, tmux_mode, timeout_ms
    raise CliUsageError(
        f"Unknown option '{token}'.",
        "Run with '--help' to see Layer 6 options, or place '--' before custom <program> flags.",
    )


def _validate_positionals_and_modes(
    positionals: Sequence[str],
    tmux_mode: str | None,
    timeout_ms: int,
    extra_args: list[str],
) -> ParsedCliArgs:
    if not positionals:
        raise CliUsageError(
            "Missing required <program> argument.",
            "Run 'just iterm2 capability_probe' or pass '--help' for full usage.",
        )
    program = positionals[0]
    if not VALID_PROGRAM_PATTERN.match(program):
        raise CliUsageError(
            f"Invalid <program> name '{program}'.",
            "Pass a valid Cargo example binary name such as 'capability_probe'.",
        )
    explicit_scenario = positionals[1] if len(positionals) >= 2 else None
    if explicit_scenario is not None and explicit_scenario not in VALID_SCENARIOS:
        raise CliUsageError(
            f"Invalid scenario '{explicit_scenario}'.",
            f"Expected one of: {', '.join(VALID_SCENARIOS)}.",
        )
    if len(positionals) > 2:
        raise CliUsageError(
            f"Unexpected positional argument '{positionals[2]}'.",
            "Use '--' before forwarding extra positional arguments to <program>.",
        )
    scenario = explicit_scenario or ("all" if program == "capability_probe" else "passthrough")
    if scenario == "matrix" and tmux_mode is not None:
        raise CliUsageError(
            "Cannot combine '--tmux' with the 'matrix' scenario.",
            "Run 'just iterm2 capability_probe matrix' without '--tmux'; matrix sweeps tmux modes itself.",
        )
    return ParsedCliArgs(False, program, scenario, tmux_mode, timeout_ms, extra_args)


def parse_cli_args(argv: Sequence[str]) -> ParsedCliArgs:
    """Parses CLI tokens for `rig/iterm2/run.sh`."""
    for token in argv:
        if token == "--":
            break
        if token in ("-h", "--help"):
            return ParsedCliArgs(True, None, None, None, DEFAULT_TIMEOUT_MS, [])
    _reject_manual_log_flag(argv)
    positionals: list[str] = []
    extra_args: list[str] = []
    tmux_mode: str | None = None
    timeout_ms = DEFAULT_TIMEOUT_MS
    idx = 0
    while idx < len(argv):
        token = argv[idx]
        if token == "--":
            extra_args.extend(argv[idx + 1 :])
            break
        if token.startswith("-"):
            idx, tmux_mode, timeout_ms = _consume_option_token(
                argv, idx, bool(positionals), tmux_mode, timeout_ms, extra_args
            )
            continue
        positionals.append(token)
        idx += 1
    _reject_manual_log_flag(extra_args)
    return _validate_positionals_and_modes(positionals, tmux_mode, timeout_ms, extra_args)


def merge_probe_args(managed_args: Sequence[str], extra_args: Sequence[str]) -> list[str]:
    """Merges managed probe flags with `extra_args`, giving `managed_args` precedence."""
    combined = [*managed_args, *extra_args]
    merged: list[str] = []
    seen_flags: set[str] = set()
    idx = 0
    while idx < len(combined):
        token = combined[idx]
        inline_flag = token.split("=", 1)[0] if "=" in token and token.index("=") > 0 else None
        if token in BOOLEAN_PROBE_FLAGS:
            if token not in seen_flags:
                seen_flags.add(token)
                merged.append(token)
            idx += 1
            continue
        if token in VALUE_PROBE_FLAGS:
            val = combined[idx + 1] if idx + 1 < len(combined) else None
            if token not in seen_flags:
                seen_flags.add(token)
                merged.append(token)
                if val is not None:
                    merged.append(val)
            idx += 2
            continue
        if inline_flag in VALUE_PROBE_FLAGS:
            if inline_flag not in seen_flags:
                seen_flags.add(inline_flag)
                merged.append(token)
            idx += 1
            continue
        merged.append(token)
        idx += 1
    return merged
