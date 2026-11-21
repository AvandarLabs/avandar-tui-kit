#!/usr/bin/env bash
#
# Layer 3 runner: builds a workspace example binary and drives it inside
# headless Playwright Chromium xterm.js 6.0.0, emitting a single JSON report on
# stdout while keeping all progress and diagnostics on stderr.

set -euo pipefail

fail_missing_node() {
  local color_error="" color_warn="" color_reset=""
  if [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != "dumb" ] && [ -t 2 ]; then
    color_error=$'\033[31m'
    color_warn=$'\033[33m'
    color_reset=$'\033[0m'
  fi
  printf '%s❌ run.sh: Could not find '\''node'\'' in PATH.%s\n' "$color_error" "$color_reset" >&2
  printf '  %s⚠️ Install Node.js (>= 20) and ensure '\''node'\'' is in PATH, then retry.%s\n' "$color_warn" "$color_reset" >&2
  exit 1
}

command -v node >/dev/null 2>&1 || fail_missing_node

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec node "$script_directory/src/harness.mjs" "$@"
