#!/usr/bin/env bash
#
# Layer 4 runner: builds a workspace example binary and drives it inside a
# private tmux server attached to headless Playwright Chromium xterm.js 6.0.0,
# emitting a single JSON report on stdout while keeping diagnostics on stderr.

set -euo pipefail

has_help_flag_before_double_dash() {
  local token
  for token in "$@"; do
    case "$token" in
      --) return 1 ;;
      -h|--help) return 0 ;;
    esac
  done
  return 1
}

fail_missing_dependency() {
  local binary_name="$1" recovery_hint="$2"
  local color_error="" color_warn="" color_reset=""
  if [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != "dumb" ] && [ -t 2 ]; then
    color_error=$'\033[31m'
    color_warn=$'\033[33m'
    color_reset=$'\033[0m'
  fi
  printf '%s❌ run-tmux.sh: Could not find '\''%s'\'' in PATH.%s\n' \
    "$color_error" "$binary_name" "$color_reset" >&2
  printf '  %s⚠️ %s%s\n' "$color_warn" "$recovery_hint" "$color_reset" >&2
  exit 1
}

command -v node >/dev/null 2>&1 || fail_missing_dependency \
  "node" "Install Node.js (>= 20) and ensure 'node' is in PATH, then retry."

if ! has_help_flag_before_double_dash "$@"; then
  command -v tmux >/dev/null 2>&1 || fail_missing_dependency \
    "tmux" "Install tmux (>= 3.3) and ensure 'tmux' is in PATH, then retry."
fi

source_path="${BASH_SOURCE[0]}"
case "$source_path" in
  */*) script_directory="$(cd "${source_path%/*}" && pwd)" ;;
  *) script_directory="$(pwd)" ;;
esac

exec node "$script_directory/src/tmux_session.mjs" "$@"
