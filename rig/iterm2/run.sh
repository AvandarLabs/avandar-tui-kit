#!/usr/bin/env bash
#
# Layer 6 runner: boots the headless tuikit-macos Tart VM over loopback VNC,
# drives iTerm2 (alone, in tmux, or in tmux -CC), and emits a single JSON report
# on stdout while keeping all progress and diagnostics on stderr.

set -euo pipefail

COLOR_RESET=""; COLOR_BOLD=""; COLOR_HEADING=""; COLOR_ACCENT=""
COLOR_MUTED=""; COLOR_SUCCESS=""; COLOR_WARN=""; COLOR_ERROR=""

setup_theme() {
  local file_descriptor="$1"
  if [ -n "${NO_COLOR:-}" ] || [ "${TERM:-}" = "dumb" ] || [ ! -t "$file_descriptor" ]; then
    COLOR_RESET=""; COLOR_BOLD=""; COLOR_HEADING=""; COLOR_ACCENT=""
    COLOR_MUTED=""; COLOR_SUCCESS=""; COLOR_WARN=""; COLOR_ERROR=""
  else
    COLOR_RESET=$'\033[0m'; COLOR_BOLD=$'\033[1m'; COLOR_HEADING=$'\033[1;36m'
    COLOR_ACCENT=$'\033[36m'; COLOR_MUTED=$'\033[2m'; COLOR_SUCCESS=$'\033[32m'
    COLOR_WARN=$'\033[33m'; COLOR_ERROR=$'\033[31m'
  fi
}

print_usage_block() {
  printf '%s\n' \
    "${COLOR_HEADING}Usage:${COLOR_RESET}" \
    "  ${COLOR_BOLD}just iterm2${COLOR_RESET} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]" \
    "  ${COLOR_BOLD}./rig/iterm2/run.sh${COLOR_RESET} <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [--timeout-ms <ms>] [-- extra_args...]" \
    "" \
    "${COLOR_HEADING}Purpose:${COLOR_RESET}" \
    "  Boot the headless tuikit-macos Tart VM over loopback VNC, drive iTerm2 (alone, in tmux," \
    "  or in tmux -CC), and emit a single machine-parseable JSON report on stdout (Layer 6)." \
    "" \
    "${COLOR_HEADING}Arguments:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}<program>${COLOR_RESET}              Cargo example binary name (for example: capability_probe)" \
    "  ${COLOR_ACCENT}[scenario]${COLOR_RESET}             Scenario name (default: 'all' for capability_probe, 'mouse' for mouse_lab, 'passthrough' otherwise)" \
    "" \
    "${COLOR_HEADING}Scenarios:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}all${COLOR_RESET}                    Run batch, osc52, interactive, and panic in one cold-booted VM" \
    "  ${COLOR_ACCENT}batch${COLOR_RESET}                  Run non-interactive capability_probe sections and verify teardown" \
    "  ${COLOR_ACCENT}osc52${COLOR_RESET}                  Verify OSC 52 clipboard and tmux buffer behavior" \
    "  ${COLOR_ACCENT}interactive${COLOR_RESET}            Drive VNC pointer, keyboard, paste, focus, and Kitty Enter checks" \
    "  ${COLOR_ACCENT}panic${COLOR_RESET}                  Trigger F12 panic and verify terminal mode teardown" \
    "  ${COLOR_ACCENT}matrix${COLOR_RESET}                 Sweep M0 rows 1, 4, 5, 6, 7, 9, 10, 11, 12, 13 and restore guest defaults" \
    "  ${COLOR_ACCENT}mouse, x1..x14${COLOR_RESET}        Run mouse_lab VM scenarios (default for mouse_lab)" \
    "  ${COLOR_ACCENT}passthrough${COLOR_RESET}            Run a custom <program> with --log managed by the driver" \
    "" \
    "${COLOR_HEADING}Options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}-h, --help${COLOR_RESET}             Show this help text on stdout and exit 0" \
    "  ${COLOR_ACCENT}--tmux <mode>${COLOR_RESET}          Run inside private tmux: stock, full, cc-stock, or cc-full" \
    "  ${COLOR_ACCENT}--timeout-ms <ms>${COLOR_RESET}      Bounded per-phase condition timeout in milliseconds (default: 45000)" \
    "  ${COLOR_ACCENT}--${COLOR_RESET}                     Forward all remaining arguments verbatim to <program>" \
    "  ${COLOR_MUTED}(Note: --log <path> is managed automatically by the driver and must not be passed manually.)${COLOR_RESET}" \
    "" \
    "${COLOR_HEADING}Streams:${COLOR_RESET}" \
    "  stdout                 Single machine-parseable JSON report object" \
    "  stderr                 VM/scenario progress, warnings, errors, and final status summary" \
    "" \
    "${COLOR_HEADING}Examples:${COLOR_RESET}" \
    "  just iterm2 capability_probe" \
    "  just iterm2 capability_probe matrix" \
    "  just iterm2 mouse_lab mouse -- --query-modes" \
    "  just iterm2 --tmux full capability_probe all" \
    "  ./rig/iterm2/run.sh capability_probe all --tmux cc-full"
}

for scanned_arg in "$@"; do
  case "$scanned_arg" in
    --) break ;;
    -h|--help) setup_theme 1; print_usage_block; exit 0 ;;
  esac
done

setup_theme 2

if ! command -v python3 >/dev/null 2>&1; then
  printf '%s❌ run.sh: Could not find '\''python3'\'' in PATH.%s\n' "$COLOR_ERROR" "$COLOR_RESET" >&2
  printf '  %s⚠️ Install Python 3 and ensure '\''python3'\'' is available in PATH, then retry.%s\n' "$COLOR_WARN" "$COLOR_RESET" >&2
  exit 1
fi

script_source="${BASH_SOURCE[0]}"
script_dir_part="${script_source%/*}"
if [ "$script_dir_part" = "$script_source" ]; then
  script_dir_part="."
fi
script_directory="$(cd "$script_dir_part" && pwd)"

PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$script_directory${PYTHONPATH:+:$PYTHONPATH}" exec python3 -B -m driver.main "$@"
