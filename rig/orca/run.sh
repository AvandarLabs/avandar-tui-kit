#!/usr/bin/env bash
#
# Layer 5 runner: builds a workspace example binary and runs it inside an
# unfocused live Orca terminal (alone or inside a private tmux server),
# emitting captured JSONL section and summary records to stdout while keeping
# all progress and diagnostics on stderr.

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
    "  ${COLOR_BOLD}just orca${COLOR_RESET} [--tmux <stock|full>] <program> [--batch | --interactive] [--timeout-ms <ms>] [program_args...]" \
    "  ${COLOR_BOLD}./rig/orca/run.sh${COLOR_RESET} [--tmux <stock|full>] <program> [--batch | --interactive] [--timeout-ms <ms>] [program_args...]" \
    "" \
    "${COLOR_HEADING}Purpose:${COLOR_RESET}" \
    "  Build a workspace example binary and run it inside an unfocused live Orca" \
    "  terminal (Layer 5), streaming its JSONL section and summary records to stdout." \
    "" \
    "${COLOR_HEADING}Arguments:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}<program>${COLOR_RESET}              Cargo example binary name (for example: capability_probe)" \
    "" \
    "${COLOR_HEADING}Options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}-h, --help${COLOR_RESET}             Show this help text on stdout and exit 0" \
    "  ${COLOR_ACCENT}--tmux <stock|full>${COLOR_RESET}    Run <program> inside a private tmux server using rig/tmux/<mode>.conf" \
    "  ${COLOR_ACCENT}--batch${COLOR_RESET}                Pass --batch to <program> so it exits after automated sections (default)" \
    "  ${COLOR_ACCENT}--interactive${COLOR_RESET}          Run <program> without auto-appending --batch" \
    "  ${COLOR_ACCENT}--timeout-ms <ms>${COLOR_RESET}      Maximum wait time in milliseconds for terminal exit (default: 20000)" \
    "  ${COLOR_ACCENT}--${COLOR_RESET}                     Forward all remaining arguments verbatim to <program>" \
    "" \
    "${COLOR_HEADING}Forwarded capability_probe options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}--osc52 <token>${COLOR_RESET}        Emit OSC 52 clipboard sequence(s) for <token>" \
    "  ${COLOR_ACCENT}--osc52-form <form>${COLOR_RESET}    Select OSC 52 form: auto (default), plain, tmux, or both" \
    "  ${COLOR_ACCENT}--push-kitty${COLOR_RESET}           Push Kitty keyboard disambiguate flags during interactive mode" \
    "  ${COLOR_MUTED}(Note: --log <path> is managed automatically by run.sh and must not be passed manually.)${COLOR_RESET}" \
    "" \
    "${COLOR_HEADING}Streams:${COLOR_RESET}" \
    "  stdout                 JSONL \"section\" and \"summary\" records captured from <program>" \
    "  stderr                 Build/launch progress, warnings, errors, and final status summary" \
    "" \
    "${COLOR_HEADING}Examples:${COLOR_RESET}" \
    "  just orca capability_probe" \
    "  just orca --tmux full capability_probe" \
    "  just orca capability_probe --batch --osc52 \"tuikit-orca-osc52-probe\"" \
    "  ./rig/orca/run.sh capability_probe --timeout-ms 30000"
}

for scanned_argument in "$@"; do
  case "$scanned_argument" in
    --) break ;;
    -h|--help) setup_theme 1; print_usage_block; exit 0 ;;
  esac
done

setup_theme 2

log_info() { printf '%s==>%s %s\n' "$COLOR_ACCENT" "$COLOR_RESET" "$1" >&2; }
log_success() { printf '%s✅ %s%s\n' "$COLOR_SUCCESS" "$1" "$COLOR_RESET" >&2; }

fail_error() {
  printf '%s❌ run.sh: %s%s\n' "$COLOR_ERROR" "$1" "$COLOR_RESET" >&2
  printf '  %s⚠️ %s%s\n' "$COLOR_WARN" "$2" "$COLOR_RESET" >&2
  exit "${3:-1}"
}

fail_usage() { print_usage_block >&2; printf '\n' >&2; fail_error "$1" "$2" 2; }

program_name=""
tmux_mode=""
is_interactive=0
has_batch_flag=0
timeout_milliseconds="20000"
forwarded_arguments=()

set_timeout_milliseconds() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]] || fail_usage \
    "Option '--timeout-ms' requires a positive integer value in milliseconds, got '$1'." \
    "Pass a positive integer, for example: just orca capability_probe --timeout-ms 20000"
  timeout_milliseconds="$1"
}

set_tmux_mode() {
  [ -z "$tmux_mode" ] || fail_usage "Duplicate '--tmux' option." \
    "Pass '--tmux' at most once with either 'stock' or 'full'."
  case "$1" in
    stock|full) tmux_mode="$1" ;;
    *) fail_usage "Option '--tmux' requires 'stock' or 'full', got '$1'." \
      "Pass '--tmux stock' or '--tmux full', for example: just orca --tmux full capability_probe" ;;
  esac
}

set_program_name() {
  [[ "$1" =~ ^[A-Za-z0-9_-]+$ ]] || fail_usage "Invalid <program> name '$1'." \
    "Pass a valid Cargo example binary name such as 'capability_probe'."
  program_name="$1"
}

consume_passthrough_arguments() {
  [ -n "$program_name" ] || fail_usage "Missing <program> before '--'." \
    "Put the example name before '--', for example: just orca capability_probe -- --batch"
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --batch) has_batch_flag=1 ;;
      --log|--log=*) fail_usage \
        "Do not pass '--log' manually; run.sh manages the JSONL log path automatically." \
        "Omit '--log' and read the captured JSONL records from stdout instead." ;;
    esac
    forwarded_arguments+=("$1"); shift
  done
}

parse_cli_arguments() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --interactive) is_interactive=1; shift ;;
      --batch) has_batch_flag=1; forwarded_arguments+=("--batch"); shift ;;
      --timeout-ms)
        [ "$#" -ge 2 ] || fail_usage "Option '--timeout-ms' requires a positive integer value." \
          "Pass a positive integer, for example: just orca capability_probe --timeout-ms 20000"
        set_timeout_milliseconds "$2"; shift 2 ;;
      --timeout-ms=*) set_timeout_milliseconds "${1#--timeout-ms=}"; shift ;;
      --tmux)
        [ "$#" -ge 2 ] || fail_usage "Option '--tmux' requires 'stock' or 'full'." \
          "Pass '--tmux stock' or '--tmux full', for example: just orca --tmux full capability_probe"
        set_tmux_mode "$2"; shift 2 ;;
      --tmux=*) set_tmux_mode "${1#--tmux=}"; shift ;;
      --log|--log=*) fail_usage \
        "Do not pass '--log' manually; run.sh manages the JSONL log path automatically." \
        "Omit '--log' and read the captured JSONL records from stdout instead." ;;
      --) shift; consume_passthrough_arguments "$@"; break ;;
      -*)
        [ -n "$program_name" ] || fail_usage "Expected <program> or a runner option before '$1'." \
          "Put the example name first, for example: just orca capability_probe"
        forwarded_arguments+=("$1"); shift ;;
      *)
        if [ -z "$program_name" ]; then set_program_name "$1"; else forwarded_arguments+=("$1"); fi
        shift ;;
    esac
  done
}

parse_cli_arguments "$@"
[ -n "$program_name" ] || fail_usage "Missing required <program> argument." \
  "Run 'just orca capability_probe' or pass '--help' for full usage."
if [ "$is_interactive" -eq 1 ] && [ "$has_batch_flag" -eq 1 ]; then
  fail_usage "Cannot combine '--batch' and '--interactive'." \
    "Pass either '--batch' (default) or '--interactive', not both."
fi
[ "$is_interactive" -eq 1 ] || [ "$has_batch_flag" -eq 1 ] || forwarded_arguments+=("--batch")

resolve_orca_binary() {
  if [ -n "${ORCA_CLI_COMMAND:-}" ]; then command -v "$ORCA_CLI_COMMAND" 2>/dev/null; return $?; fi
  if [ -n "${ORCA_DEV_REPO_ROOT:-}" ] && command -v orca-dev >/dev/null 2>&1; then
    command -v orca-dev; return 0
  fi
  local system_name path_candidates saved_ifs="$IFS"
  system_name="$(uname -s)"
  path_candidates="$(type -ap orca 2>/dev/null || true)"
  IFS=$'\n'
  # Ignore /usr/bin/orca on Linux because that path belongs to the GNOME screen reader.
  for candidate_path in $path_candidates; do
    IFS="$saved_ifs"
    if [ -n "$candidate_path" ] \
      && ! { [ "$system_name" = "Linux" ] && [ "$candidate_path" = "/usr/bin/orca" ]; }; then
      printf '%s\n' "$candidate_path"; return 0
    fi
  done
  IFS="$saved_ifs"
  if [ "$system_name" = "Linux" ] && command -v orca-ide >/dev/null 2>&1; then
    command -v orca-ide; return 0
  fi
  if [ -n "${HOME:-}" ] && [ -x "${HOME}/.orca/cli/bin/orca" ]; then
    printf '%s\n' "${HOME}/.orca/cli/bin/orca"; return 0
  fi
  return 1
}

orca_binary=""
orca_binary="$(resolve_orca_binary)" || fail_error \
  "Could not find the 'orca' CLI in PATH or ~/.orca/cli/bin/orca." \
  "Ensure Orca is installed and add ~/.orca/cli/bin to PATH, then retry."

tmux_binary=""
if [ -n "$tmux_mode" ]; then
  tmux_binary="$(command -v tmux 2>/dev/null || true)"
  [ -n "$tmux_binary" ] || fail_error \
    "Could not find 'tmux' in PATH for '--tmux $tmux_mode'." \
    "Install tmux (>= 3.3) and ensure it is in PATH, then retry."
fi

command -v cargo >/dev/null 2>&1 || fail_error \
  "Could not find 'cargo' in PATH." \
  "Install the Rust toolchain via rustup and ensure ~/.cargo/bin is in PATH."

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../.." && pwd)"
[ -f "$repository_root/Cargo.toml" ] || fail_error \
  "Workspace Cargo.toml not found at '$repository_root/Cargo.toml'." \
  "Run this script from an intact avandar-tui-kit checkout."

tmux_config_path=""
if [ -n "$tmux_mode" ]; then
  tmux_config_path="$repository_root/rig/tmux/${tmux_mode}.conf"
  [ -f "$tmux_config_path" ] || fail_error \
    "tmux configuration file not found at '$tmux_config_path'." \
    "Verify rig/tmux/${tmux_mode}.conf exists in '$repository_root'."
fi

log_info "Building workspace example '$program_name'..."
cargo build --manifest-path "$repository_root/Cargo.toml" --workspace --example "$program_name" >&2 \
  || fail_error "Failed to build example '$program_name'." \
    "Check available example names under rig/probe/examples/ and fix any compile errors above."

raw_target_directory="${CARGO_TARGET_DIR:-$repository_root/target}"
[ -d "$raw_target_directory" ] || fail_error \
  "Cargo target directory was not found at '$raw_target_directory'." \
  "Verify CARGO_TARGET_DIR and check that 'cargo build' succeeded."
target_directory="$(cd "$raw_target_directory" && pwd)"
binary_path="$target_directory/debug/examples/$program_name"
[ -x "$binary_path" ] || fail_error \
  "Compiled example binary was not found at '$binary_path'." \
  "Verify that '$program_name' is declared as an example in the Cargo workspace."

build_shell_quoted_string() {
  local remaining_text="$1" quoted_output="'"
  while [[ "$remaining_text" == *"'"* ]]; do
    quoted_output+="${remaining_text%%\'*}'\\''"
    remaining_text="${remaining_text#*\'}"
  done
  printf '%s' "${quoted_output}${remaining_text}'"
}

extract_terminal_handle() {
  local json_payload="$1"
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json_payload" | python3 -c '
import json, sys
try: parsed = json.load(sys.stdin)
except Exception: sys.exit(1)
if not isinstance(parsed, dict) or parsed.get("ok") is False: sys.exit(1)
res = parsed.get("result") if isinstance(parsed.get("result"), dict) else parsed
term = res.get("terminal") if isinstance(res.get("terminal"), dict) else res
handle = term.get("handle", "")
if not isinstance(handle, str) or not handle: sys.exit(1)
print(handle)
'
    return $?
  fi
  ! printf '%s' "$json_payload" | grep -Eq '"ok"[[:space:]]*:[[:space:]]*false' || return 1
  local handle
  handle="$(printf '%s\n' "$json_payload" | grep -Eo '"handle"[[:space:]]*:[[:space:]]*"[^"]+"' | head -n 1 | sed -E 's/.*"([^"]+)"$/\1/')"
  [ -n "$handle" ] && printf '%s\n' "$handle"
}

check_wait_satisfied() {
  local json_payload="$1"
  if command -v python3 >/dev/null 2>&1; then
    printf '%s' "$json_payload" | python3 -c '
import json, sys
try: parsed = json.load(sys.stdin)
except Exception: sys.exit(1)
if not isinstance(parsed, dict) or parsed.get("ok") is False: sys.exit(1)
res = parsed.get("result") if isinstance(parsed.get("result"), dict) else parsed
wait = res.get("wait") if isinstance(res.get("wait"), dict) else res
if wait.get("satisfied") is False: sys.exit(1)
'
    return $?
  fi
  ! printf '%s' "$json_payload" | grep -Eq '"(ok|satisfied)"[[:space:]]*:[[:space:]]*false'
}

scratch_parent="${TMPDIR:-/tmp}"; scratch_parent="${scratch_parent%/}"
scratch_directory=""; log_file=""; exit_code_file=""
terminal_handle=""; tmux_socket=""; has_succeeded=0

cleanup_resources() {
  if [ -n "$tmux_socket" ] && [ -n "$tmux_binary" ]; then
    "$tmux_binary" -L "$tmux_socket" kill-server >/dev/null 2>&1 || true
    rm -f "/tmp/tmux-$(id -u)/$tmux_socket" "${TMUX_TMPDIR:-/tmp}/tmux-$(id -u)/$tmux_socket" 2>/dev/null || true
    tmux_socket=""
  fi
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

probe_command="$(build_shell_quoted_string "$binary_path") --log $(build_shell_quoted_string "$log_file")"
for argument_item in ${forwarded_arguments[@]+"${forwarded_arguments[@]}"}; do
  probe_command+=" $(build_shell_quoted_string "$argument_item")"
done
probe_command+="; printf '%s\n' \"\$?\" > $(build_shell_quoted_string "$exit_code_file")"

if [ -n "$tmux_mode" ]; then
  tmux_socket="tuikit-orca-$$-${RANDOM:-0}"
  remote_command="TMUX= $(build_shell_quoted_string "$tmux_binary") -L $(build_shell_quoted_string "$tmux_socket") -f $(build_shell_quoted_string "$tmux_config_path") new-session $(build_shell_quoted_string "$probe_command"); exit"
  log_info "Launching '$program_name' inside tmux ($tmux_mode) in an unfocused Orca terminal..."
else
  remote_command="$probe_command; exit"
  log_info "Launching '$program_name' in an unfocused Orca terminal..."
fi

create_response="$("$orca_binary" terminal create --worktree "path:$repository_root" --title "probe:$program_name" --command "$remote_command" --json)" \
  || fail_error "orca terminal create failed." \
    "Verify Orca is running with 'orca status --json' (preserved scratch dir: $scratch_directory)."

terminal_handle="$(extract_terminal_handle "$create_response")" \
  || fail_error "Could not extract terminal handle from 'orca terminal create' JSON output." \
    "Inspect Orca status with 'orca status --json' and preserved scratch dir '$scratch_directory'."

log_info "Waiting for '$terminal_handle' to exit (timeout: ${timeout_milliseconds}ms)..."
wait_response="$("$orca_binary" terminal wait --terminal "$terminal_handle" --for exit --timeout-ms "$timeout_milliseconds" --json)" \
  || fail_error "Timed out or failed waiting for Orca terminal '$terminal_handle' to exit." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."

check_wait_satisfied "$wait_response" \
  || fail_error "Orca terminal '$terminal_handle' did not satisfy the exit condition within ${timeout_milliseconds}ms." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."

"$orca_binary" terminal close --terminal "$terminal_handle" --tab --json >/dev/null 2>&1 || true
terminal_handle=""

[ -f "$exit_code_file" ] || fail_error \
  "Program '$program_name' exited without recording an exit status file." \
  "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."

recorded_exit_code="$(tr -d '[:space:]' < "$exit_code_file")"
[ "$recorded_exit_code" = "0" ] || fail_error \
  "Program '$program_name' exited with non-zero status '$recorded_exit_code'." \
  "Inspect preserved probe log at '$log_file'."

if [ ! -f "$log_file" ] || [ ! -f "$log_file.ready" ]; then
  fail_error "Program '$program_name' did not create the ready sentinel '$log_file.ready'." \
    "Inspect preserved probe artifacts in '$scratch_directory' (log: '$log_file')."
fi
grep -q '"kind":"ready"' "$log_file" || fail_error \
  "Probe log '$log_file' is missing the '\"kind\":\"ready\"' record." \
  "Inspect preserved probe log at '$log_file'."

grep -E '^\{"kind":"(section|summary)"' "$log_file" || true

has_succeeded=1
log_success "Completed '$program_name' in unfocused Orca terminal."
