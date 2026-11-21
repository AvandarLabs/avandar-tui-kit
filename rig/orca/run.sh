#!/usr/bin/env bash
#
# Layer 5 runner: builds a workspace example binary and runs it inside an
# unfocused live Orca terminal, emitting captured JSONL section and summary
# records to stdout while keeping all progress and diagnostics on stderr.

set -euo pipefail

COLOR_RESET=""
COLOR_BOLD=""
COLOR_HEADING=""
COLOR_ACCENT=""
COLOR_MUTED=""
COLOR_SUCCESS=""
COLOR_WARN=""
COLOR_ERROR=""

setup_theme() {
  local file_descriptor="$1"
  if [ -n "${NO_COLOR:-}" ] || [ "${TERM:-}" = "dumb" ] || [ ! -t "$file_descriptor" ]; then
    COLOR_RESET=""
    COLOR_BOLD=""
    COLOR_HEADING=""
    COLOR_ACCENT=""
    COLOR_MUTED=""
    COLOR_SUCCESS=""
    COLOR_WARN=""
    COLOR_ERROR=""
  else
    COLOR_RESET=$'\033[0m'
    COLOR_BOLD=$'\033[1m'
    COLOR_HEADING=$'\033[1;36m'
    COLOR_ACCENT=$'\033[36m'
    COLOR_MUTED=$'\033[2m'
    COLOR_SUCCESS=$'\033[32m'
    COLOR_WARN=$'\033[33m'
    COLOR_ERROR=$'\033[31m'
  fi
}

print_usage_block() {
  cat <<EOF
${COLOR_HEADING}Usage:${COLOR_RESET}
  ${COLOR_BOLD}just orca${COLOR_RESET} <program> [--batch | --interactive] [--timeout-ms <ms>] [program_args...]
  ${COLOR_BOLD}./rig/orca/run.sh${COLOR_RESET} <program> [--batch | --interactive] [--timeout-ms <ms>] [program_args...]

${COLOR_HEADING}Purpose:${COLOR_RESET}
  Build a workspace example binary and run it inside an unfocused live Orca
  terminal (Layer 5), streaming its JSONL section and summary records to stdout.

${COLOR_HEADING}Arguments:${COLOR_RESET}
  ${COLOR_ACCENT}<program>${COLOR_RESET}              Cargo example binary name (for example: capability_probe)

${COLOR_HEADING}Options:${COLOR_RESET}
  ${COLOR_ACCENT}-h, --help${COLOR_RESET}             Show this help text on stdout and exit 0
  ${COLOR_ACCENT}--batch${COLOR_RESET}                Pass --batch to <program> so it exits after automated sections (default)
  ${COLOR_ACCENT}--interactive${COLOR_RESET}          Run <program> without auto-appending --batch
  ${COLOR_ACCENT}--timeout-ms <ms>${COLOR_RESET}      Maximum wait time in milliseconds for terminal exit (default: 20000)
  ${COLOR_ACCENT}--${COLOR_RESET}                     Forward all remaining arguments verbatim to <program>

${COLOR_HEADING}Forwarded capability_probe options:${COLOR_RESET}
  ${COLOR_ACCENT}--osc52 <token>${COLOR_RESET}        Emit plain (and in tmux, passthrough) OSC 52 sequence for <token>
  ${COLOR_ACCENT}--push-kitty${COLOR_RESET}           Push Kitty keyboard disambiguate flags during interactive mode
  ${COLOR_MUTED}(Note: --log <path> is managed automatically by run.sh and must not be passed manually.)${COLOR_RESET}

${COLOR_HEADING}Streams:${COLOR_RESET}
  stdout                 JSONL "section" and "summary" records captured from <program>
  stderr                 Build/launch progress, warnings, errors, and final status summary

${COLOR_HEADING}Examples:${COLOR_RESET}
  just orca capability_probe
  just orca capability_probe --batch --osc52 "tuikit-orca-osc52-probe"
  ./rig/orca/run.sh capability_probe --timeout-ms 30000
EOF
}

for scanned_argument in "$@"; do
  case "$scanned_argument" in
    --)
      break
      ;;
    -h|--help)
      setup_theme 1
      print_usage_block
      exit 0
      ;;
  esac
done

setup_theme 2

log_info() {
  printf '%s==>%s %s\n' "$COLOR_ACCENT" "$COLOR_RESET" "$1" >&2
}

log_success() {
  printf '%s✅ %s%s\n' "$COLOR_SUCCESS" "$1" "$COLOR_RESET" >&2
}

fail_error() {
  local error_message="$1"
  local recovery_action="$2"
  local exit_status="${3:-1}"
  printf '%s❌ run.sh: %s%s\n' "$COLOR_ERROR" "$error_message" "$COLOR_RESET" >&2
  printf '  %s⚠️ %s%s\n' "$COLOR_WARN" "$recovery_action" "$COLOR_RESET" >&2
  exit "$exit_status"
}

fail_usage() {
  local error_message="$1"
  local recovery_action="$2"
  print_usage_block >&2
  printf '\n' >&2
  fail_error "$error_message" "$recovery_action" 2
}

if [ "$#" -eq 0 ]; then
  fail_usage \
    "Missing required <program> argument." \
    "Run 'just orca capability_probe' or pass '--help' for full usage."
fi

program_name="$1"
case "$program_name" in
  -*)
    fail_usage \
      "Expected <program> as the first argument, got '$program_name'." \
      "Put the example name first, for example: just orca capability_probe"
    ;;
esac

if [[ ! "$program_name" =~ ^[A-Za-z0-9_-]+$ ]]; then
  fail_usage \
    "Invalid <program> name '$program_name'." \
    "Pass a valid Cargo example binary name such as 'capability_probe'."
fi
shift

is_interactive=0
has_batch_flag=0
timeout_milliseconds="20000"
forwarded_arguments=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --interactive)
      is_interactive=1
      shift
      ;;
    --batch)
      has_batch_flag=1
      forwarded_arguments+=("--batch")
      shift
      ;;
    --timeout-ms)
      if [ "$#" -lt 2 ] || [[ ! "${2:-}" =~ ^[1-9][0-9]*$ ]]; then
        fail_usage \
          "Option '--timeout-ms' requires a positive integer value in milliseconds." \
          "Pass a positive integer, for example: just orca capability_probe --timeout-ms 20000"
      fi
      timeout_milliseconds="$2"
      shift 2
      ;;
    --timeout-ms=*)
      timeout_value="${1#--timeout-ms=}"
      if [[ ! "$timeout_value" =~ ^[1-9][0-9]*$ ]]; then
        fail_usage \
          "Option '--timeout-ms' requires a positive integer value in milliseconds, got '$timeout_value'." \
          "Pass a positive integer, for example: just orca capability_probe --timeout-ms 20000"
      fi
      timeout_milliseconds="$timeout_value"
      shift
      ;;
    --log|--log=*)
      fail_usage \
        "Do not pass '--log' manually; run.sh manages the JSONL log path automatically." \
        "Omit '--log' and read the captured JSONL records from stdout instead."
      ;;
    --)
      shift
      while [ "$#" -gt 0 ]; do
        case "$1" in
          --batch)
            has_batch_flag=1
            ;;
          --log|--log=*)
            fail_usage \
              "Do not pass '--log' manually; run.sh manages the JSONL log path automatically." \
              "Omit '--log' and read the captured JSONL records from stdout instead."
            ;;
        esac
        forwarded_arguments+=("$1")
        shift
      done
      ;;
    *)
      forwarded_arguments+=("$1")
      shift
      ;;
  esac
done

if [ "$is_interactive" -eq 1 ] && [ "$has_batch_flag" -eq 1 ]; then
  fail_usage \
    "Cannot combine '--batch' and '--interactive'." \
    "Pass either '--batch' (default) or '--interactive', not both."
fi

if [ "$is_interactive" -eq 0 ] && [ "$has_batch_flag" -eq 0 ]; then
  forwarded_arguments+=("--batch")
fi

resolve_orca_binary() {
  if [ -n "${ORCA_CLI_COMMAND:-}" ]; then
    command -v "$ORCA_CLI_COMMAND" 2>/dev/null
    return $?
  fi
  if [ -n "${ORCA_DEV_REPO_ROOT:-}" ] && command -v orca-dev >/dev/null 2>&1; then
    command -v orca-dev
    return 0
  fi
  local candidate_path=""
  if candidate_path="$(command -v orca 2>/dev/null)"; then
    if [ "$(uname -s)" = "Linux" ] && [ -z "${ORCA_SESSION_ID:-}" ] && [ "$candidate_path" = "/usr/bin/orca" ]; then
      command -v orca-ide 2>/dev/null
      return $?
    fi
    printf '%s\n' "$candidate_path"
    return 0
  fi
  if [ "$(uname -s)" = "Linux" ] && command -v orca-ide >/dev/null 2>&1; then
    command -v orca-ide
    return 0
  fi
  if [ -n "${HOME:-}" ] && [ -x "${HOME}/.orca/cli/bin/orca" ]; then
    printf '%s\n' "${HOME}/.orca/cli/bin/orca"
    return 0
  fi
  return 1
}

orca_binary=""
if ! orca_binary="$(resolve_orca_binary)"; then
  fail_error \
    "Could not find the 'orca' CLI in PATH or ~/.orca/cli/bin/orca." \
    "Ensure Orca is installed and add ~/.orca/cli/bin to PATH, then retry."
fi

if ! command -v cargo >/dev/null 2>&1; then
  fail_error \
    "Could not find 'cargo' in PATH." \
    "Install the Rust toolchain via rustup and ensure ~/.cargo/bin is in PATH."
fi

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../.." && pwd)"

if [ ! -f "$repository_root/Cargo.toml" ]; then
  fail_error \
    "Workspace Cargo.toml not found at '$repository_root/Cargo.toml'." \
    "Run this script from an intact avandar-tui-kit checkout."
fi

log_info "Building workspace example '$program_name'..."
if ! cargo build --manifest-path "$repository_root/Cargo.toml" --workspace --example "$program_name" >&2; then
  fail_error \
    "Failed to build example '$program_name'." \
    "Check available example names under rig/probe/examples/ and fix any compile errors above."
fi

target_directory="${CARGO_TARGET_DIR:-$repository_root/target}"
binary_path="$target_directory/debug/examples/$program_name"
if [ ! -x "$binary_path" ]; then
  fail_error \
    "Compiled example binary was not found at '$binary_path'." \
    "Verify that '$program_name' is declared as an example in the Cargo workspace."
fi

build_shell_quoted_string() {
  local remaining_text="$1"
  local quoted_output="'"
  while [[ "$remaining_text" == *"'"* ]]; do
    quoted_output+="${remaining_text%%\'*}'\\''"
    remaining_text="${remaining_text#*\'}"
  done
  quoted_output+="${remaining_text}'"
  printf '%s' "$quoted_output"
}

extract_terminal_handle() {
  local json_payload="$1"
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json_payload" | python3 -c '
import json, sys
try:
    parsed_json = json.load(sys.stdin)
except Exception:
    sys.exit(1)
if not isinstance(parsed_json, dict) or parsed_json.get("ok") is False:
    sys.exit(1)
result_obj = parsed_json.get("result") if isinstance(parsed_json.get("result"), dict) else parsed_json
terminal_obj = result_obj.get("terminal") if isinstance(result_obj.get("terminal"), dict) else result_obj
handle_value = terminal_obj.get("handle", "")
if not isinstance(handle_value, str) or not handle_value:
    sys.exit(1)
print(handle_value)
'
    return $?
  fi
  if printf '%s' "$json_payload" | grep -Eq '"ok"[[:space:]]*:[[:space:]]*false'; then
    return 1
  fi
  local extracted_handle=""
  extracted_handle="$(
    printf '%s\n' "$json_payload" \
      | grep -Eo '"handle"[[:space:]]*:[[:space:]]*"[^"]+"' \
      | head -n 1 \
      | sed -E 's/.*"([^"]+)"$/\1/'
  )"
  [ -n "$extracted_handle" ] && printf '%s\n' "$extracted_handle"
}

check_wait_satisfied() {
  local json_payload="$1"
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json_payload" | python3 -c '
import json, sys
try:
    parsed_json = json.load(sys.stdin)
except Exception:
    sys.exit(1)
if not isinstance(parsed_json, dict) or parsed_json.get("ok") is False:
    sys.exit(1)
result_obj = parsed_json.get("result") if isinstance(parsed_json.get("result"), dict) else parsed_json
wait_obj = result_obj.get("wait") if isinstance(result_obj.get("wait"), dict) else result_obj
if wait_obj.get("satisfied") is False:
    sys.exit(1)
'
    return $?
  fi
  ! printf '%s' "$json_payload" | grep -Eq '"(ok|satisfied)"[[:space:]]*:[[:space:]]*false'
}

scratch_parent="${TMPDIR:-/tmp}"
scratch_parent="${scratch_parent%/}"
scratch_directory=""
log_file=""
exit_code_file=""
terminal_handle=""
has_succeeded=0

cleanup_resources() {
  if [ -n "$terminal_handle" ] && [ -n "$orca_binary" ]; then
    "$orca_binary" terminal close --terminal "$terminal_handle" --tab --json >/dev/null 2>&1 || true
    terminal_handle=""
  fi
  if [ "$has_succeeded" -eq 1 ] && [ -n "$scratch_directory" ] && [ -d "$scratch_directory" ]; then
    rm -rf "$scratch_directory"
  fi
}
trap cleanup_resources EXIT

scratch_directory="$(mktemp -d "$scratch_parent/tuikit-orca.XXXXXX")"
log_file="$scratch_directory/probe.jsonl"
exit_code_file="$scratch_directory/exit_code"

remote_command="$(build_shell_quoted_string "$binary_path") --log $(build_shell_quoted_string "$log_file")"
for argument_item in ${forwarded_arguments[@]+"${forwarded_arguments[@]}"}; do
  remote_command+=" $(build_shell_quoted_string "$argument_item")"
done
remote_command+="; printf '%s\n' \"\$?\" > $(build_shell_quoted_string "$exit_code_file"); exit"

log_info "Launching '$program_name' in an unfocused Orca terminal..."
create_response=""
if ! create_response="$(
  "$orca_binary" terminal create \
    --worktree "path:$repository_root" \
    --title "probe:$program_name" \
    --command "$remote_command" \
    --json
)"; then
  fail_error \
    "orca terminal create failed." \
    "Verify Orca is running with 'orca status --json' (preserved scratch dir: $scratch_directory)."
fi

if ! terminal_handle="$(extract_terminal_handle "$create_response")"; then
  fail_error \
    "Could not extract terminal handle from 'orca terminal create' JSON output." \
    "Inspect Orca status with 'orca status --json' and preserved scratch dir '$scratch_directory'."
fi

log_info "Waiting for '$terminal_handle' to exit (timeout: ${timeout_milliseconds}ms)..."
wait_response=""
if ! wait_response="$(
  "$orca_binary" terminal wait \
    --terminal "$terminal_handle" \
    --for exit \
    --timeout-ms "$timeout_milliseconds" \
    --json
)"; then
  fail_error \
    "Timed out or failed waiting for Orca terminal '$terminal_handle' to exit." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."
fi

if ! check_wait_satisfied "$wait_response"; then
  fail_error \
    "Orca terminal '$terminal_handle' did not satisfy the exit condition within ${timeout_milliseconds}ms." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."
fi

"$orca_binary" terminal close --terminal "$terminal_handle" --tab --json >/dev/null 2>&1 || true
terminal_handle=""

if [ ! -f "$exit_code_file" ]; then
  fail_error \
    "Program '$program_name' exited without recording an exit status file." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."
fi

recorded_exit_code="$(tr -d '[:space:]' < "$exit_code_file")"
if [ "$recorded_exit_code" != "0" ]; then
  fail_error \
    "Program '$program_name' exited with non-zero status '$recorded_exit_code'." \
    "Inspect preserved probe log at '$log_file'."
fi

if [ ! -f "$log_file" ] || [ ! -f "$log_file.ready" ]; then
  fail_error \
    "Program '$program_name' did not create the ready sentinel '$log_file.ready'." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."
fi

if ! grep -q '"kind":"ready"' "$log_file"; then
  fail_error \
    "Probe log '$log_file' is missing the '\"kind\":\"ready\"' record." \
    "Inspect preserved probe log at '$log_file'."
fi

grep -E '^\{"kind":"(section|summary)"' "$log_file" || true

has_succeeded=1
log_success "Completed '$program_name' in unfocused Orca terminal."
