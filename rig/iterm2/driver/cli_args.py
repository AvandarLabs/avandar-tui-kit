"""CLI argument parsing, semantic terminal styling, and probe flag merging for Layer 6."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
import re

# Supported private tmux configuration modes accepted by `--tmux`.
VALID_TMUX_MODES: tuple[str, ...] = ("stock", "full", "cc-stock", "cc-full")

# Supported scenario names accepted by the Layer 6 CLI.
VALID_SCENARIOS: tuple[str, ...] = (
    "all",
    "batch",
    "osc52",
    "interactive",
    "panic",
    "matrix",
    "passthrough",
)

# Default bounded per-phase condition timeout in milliseconds.
DEFAULT_TIMEOUT_MS = 45000

# Boolean `capability_probe` flags deduplicated during managed argument merging.
BOOLEAN_PROBE_FLAGS = frozenset({"--batch", "--push-kitty"})

# Value-taking `capability_probe` flags deduplicated during managed argument merging.
VALUE_PROBE_FLAGS = frozenset({"--log", "--osc52", "--osc52-form"})

# Allowed Cargo example binary name characters.
VALID_PROGRAM_PATTERN = re.compile(r"^[A-Za-z0-9_-]+$")

# Pattern matching strictly positive decimal integer strings.
POSITIVE_INTEGER_PATTERN = re.compile(r"^[1-9][0-9]*$")


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


def create_cli_theme(is_tty: bool, environment: Mapping[str, str]) -> dict[str, str]:
    """Builds ANSI color tokens while honoring NO_COLOR, TERM=dumb, and non-TTY streams."""
    if not is_tty or bool(environment.get("NO_COLOR")) or environment.get("TERM") == "dumb":
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


def _help_scenarios_and_options(theme: Mapping[str, str]) -> list[str]:
    accent = theme["accent"]
    heading = theme["heading"]
    muted = theme["muted"]
    reset = theme["reset"]
    return [
        f"{heading}Scenarios:{reset}",
        f"  {accent}all{reset}                    Run batch, osc52, interactive, and panic in one cold-booted VM",
        f"  {accent}batch{reset}                  Run non-interactive capability_probe sections and verify teardown",
        f"  {accent}osc52{reset}                  Verify OSC 52 clipboard and tmux buffer behavior",
        f"  {accent}interactive{reset}            Drive VNC pointer, keyboard, paste, focus, and Kitty Enter checks",
        f"  {accent}panic{reset}                  Trigger F12 panic and verify terminal mode teardown",
        f"  {accent}matrix{reset}                 Sweep M0 rows 1, 4, 5, 6, 7, 9, 10, 11, 12, 13 and restore guest defaults",
        f"  {accent}passthrough{reset}            Run a custom <program> with --log managed by the driver",
        "",
        f"{heading}Options:{reset}",
        f"  {accent}-h, --help{reset}             Show this help text on stdout and exit 0",
        f"  {accent}--tmux <mode>{reset}          Run inside private tmux: stock, full, cc-stock, or cc-full",
        f"  {accent}--timeout-ms <ms>{reset}      Bounded per-phase condition timeout in milliseconds (default: 45000)",
        f"  {accent}--{reset}                     Forward all remaining arguments verbatim to <program>",
        f"  {muted}(Note: --log <path> is managed automatically by the driver and must not be passed manually.){reset}",
    ]


def build_help_text(theme: Mapping[str, str]) -> str:
    """Renders structured CLI help text for stdout."""
    bold = theme["bold"]
    heading = theme["heading"]
    accent = theme["accent"]
    reset = theme["reset"]
    return "\n".join(
        [
            f"{heading}Usage:{reset}",
            f"  {bold}just iterm2{reset} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]",
            f"  {bold}./rig/iterm2/run.sh{reset} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]",
            "",
            f"{heading}Purpose:{reset}",
            "  Boot the headless tuikit-macos Tart VM over loopback VNC, drive iTerm2 (alone, in tmux,",
            "  or in tmux -CC), and emit a single machine-parseable JSON report on stdout (Layer 6).",
            "",
            f"{heading}Arguments:{reset}",
            f"  {accent}<program>{reset}              Cargo example binary name (for example: capability_probe)",
            f"  {accent}[scenario]{reset}             Scenario name (default: 'all' for capability_probe, 'passthrough' otherwise)",
            "",
            *_help_scenarios_and_options(theme),
            "",
            f"{heading}Streams:{reset}",
            "  stdout                 Single machine-parseable JSON report object",
            "  stderr                 VM/scenario progress, warnings, errors, and final status summary",
            "",
            f"{heading}Examples:{reset}",
            "  just iterm2 capability_probe",
            "  just iterm2 capability_probe matrix",
            "  just iterm2 --tmux full capability_probe all",
            "  ./rig/iterm2/run.sh capability_probe all --tmux cc-full",
        ]
    )


def _parse_positive_timeout(raw_value: str | None) -> int:
    if raw_value is None or not POSITIVE_INTEGER_PATTERN.match(raw_value):
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


def _next_option_value(tokens: Sequence[str], token_index: int) -> str | None:
    if token_index + 1 < len(tokens) and tokens[token_index + 1] != "--":
        return tokens[token_index + 1]
    return None


def _consume_option_token(
    tokens: Sequence[str],
    token_index: int,
    has_program: bool,
    tmux_mode: str | None,
    timeout_ms: int,
    extra_args: list[str],
) -> tuple[int, str | None, int]:
    token = tokens[token_index]
    if token == "--tmux":
        option_value = _next_option_value(tokens, token_index)
        return token_index + 2, _parse_tmux_mode(tmux_mode, option_value), timeout_ms
    if token.startswith("--tmux="):
        return token_index + 1, _parse_tmux_mode(tmux_mode, token.split("=", 1)[1]), timeout_ms
    if token == "--timeout-ms":
        option_value = _next_option_value(tokens, token_index)
        return token_index + 2, tmux_mode, _parse_positive_timeout(option_value)
    if token.startswith("--timeout-ms="):
        return token_index + 1, tmux_mode, _parse_positive_timeout(token.split("=", 1)[1])
    if has_program and token in BOOLEAN_PROBE_FLAGS:
        extra_args.append(token)
        return token_index + 1, tmux_mode, timeout_ms
    if has_program and token in ("--osc52", "--osc52-form"):
        option_value = _next_option_value(tokens, token_index)
        if option_value is None:
            raise CliUsageError(
                f"Option '{token}' requires a value.",
                f"Pass '{token} <value>' or '{token}=<value>' before '--'.",
            )
        extra_args.extend([token, option_value])
        return token_index + 2, tmux_mode, timeout_ms
    if has_program and (token.startswith("--osc52=") or token.startswith("--osc52-form=")):
        extra_args.append(token)
        return token_index + 1, tmux_mode, timeout_ms
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
    token_index = 0
    while token_index < len(argv):
        token = argv[token_index]
        if token == "--":
            extra_args.extend(argv[token_index + 1 :])
            break
        if token.startswith("-"):
            token_index, tmux_mode, timeout_ms = _consume_option_token(
                argv, token_index, bool(positionals), tmux_mode, timeout_ms, extra_args
            )
            continue
        positionals.append(token)
        token_index += 1
    _reject_manual_log_flag(extra_args)
    return _validate_positionals_and_modes(positionals, tmux_mode, timeout_ms, extra_args)


def merge_probe_args(managed_args: Sequence[str], extra_args: Sequence[str]) -> list[str]:
    """Merges managed probe flags with `extra_args`, giving `managed_args` precedence."""
    combined = [*managed_args, *extra_args]
    merged: list[str] = []
    seen_flags: set[str] = set()
    token_index = 0
    while token_index < len(combined):
        token = combined[token_index]
        inline_flag = token.split("=", 1)[0] if "=" in token and token.index("=") > 0 else None
        if token in BOOLEAN_PROBE_FLAGS:
            if token not in seen_flags:
                seen_flags.add(token)
                merged.append(token)
            token_index += 1
            continue
        if token in VALUE_PROBE_FLAGS:
            flag_value = combined[token_index + 1] if token_index + 1 < len(combined) else None
            if token not in seen_flags:
                seen_flags.add(token)
                merged.append(token)
                if flag_value is not None:
                    merged.append(flag_value)
            token_index += 2
            continue
        if inline_flag in VALUE_PROBE_FLAGS:
            if inline_flag not in seen_flags:
                seen_flags.add(inline_flag)
                merged.append(token)
            token_index += 1
            continue
        merged.append(token)
        token_index += 1
    return merged
