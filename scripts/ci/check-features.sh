#!/usr/bin/env bash
#
# Workspace feature-matrix checker: inspects cargo metadata for every published
# crate under crates/ and runs one explicit `cargo check` command per feature
# row (--no-default-features, each declared feature alone, and --all-features).

set -euo pipefail

COLOR_RESET=""; COLOR_BOLD=""; COLOR_HEADING=""; COLOR_ACCENT=""
COLOR_SUCCESS=""; COLOR_WARN=""; COLOR_ERROR=""

setup_theme() {
  local file_descriptor="$1"
  if [ -n "${NO_COLOR:-}" ] || [ "${TERM:-}" = "dumb" ] || [ ! -t "$file_descriptor" ]; then
    COLOR_RESET=""; COLOR_BOLD=""; COLOR_HEADING=""; COLOR_ACCENT=""
    COLOR_SUCCESS=""; COLOR_WARN=""; COLOR_ERROR=""
  else
    COLOR_RESET=$'\033[0m'; COLOR_BOLD=$'\033[1m'; COLOR_HEADING=$'\033[1;36m'
    COLOR_ACCENT=$'\033[36m'; COLOR_SUCCESS=$'\033[32m'
    COLOR_WARN=$'\033[33m'; COLOR_ERROR=$'\033[31m'
  fi
}

print_usage_block() {
  printf '%s\n' \
    "${COLOR_HEADING}Usage:${COLOR_RESET}" \
    "  ${COLOR_BOLD}./scripts/ci/check-features.sh${COLOR_RESET} [--manifest-path <path>]" \
    "" \
    "${COLOR_HEADING}Purpose:${COLOR_RESET}" \
    "  Verify every published crate under crates/ compiles with no features," \
    "  each declared feature alone, and all features enabled." \
    "" \
    "${COLOR_HEADING}Options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}-h, --help${COLOR_RESET}                 Show this help text on stdout and exit 0" \
    "  ${COLOR_ACCENT}--manifest-path <path>${COLOR_RESET}     Workspace Cargo.toml path (default: <repo>/Cargo.toml)" \
    "" \
    "${COLOR_HEADING}Streams:${COLOR_RESET}" \
    "  stdout                     Single-line JSON summary of checked crates and commands" \
    "  stderr                     Echoed cargo check commands, compiler output, and hints" \
    "" \
    "${COLOR_HEADING}Examples:${COLOR_RESET}" \
    "  ./scripts/ci/check-features.sh" \
    "  just linux ./scripts/ci/check-features.sh"
}

setup_theme 2

log_info() { printf '%s==>%s %s\n' "$COLOR_ACCENT" "$COLOR_RESET" "$1" >&2; }
log_success() { printf '%s✅ %s%s\n' "$COLOR_SUCCESS" "$1" "$COLOR_RESET" >&2; }

fail_error() {
  printf '%s❌ check-features.sh: %s%s\n' "$COLOR_ERROR" "$1" "$COLOR_RESET" >&2
  printf '  %s⚠️ %s%s\n' "$COLOR_WARN" "$2" "$COLOR_RESET" >&2
  exit "${3:-1}"
}

fail_usage() { print_usage_block >&2; printf '\n' >&2; fail_error "$1" "$2" 2; }

manifest_path=""

parse_arguments() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -h|--help) setup_theme 1; print_usage_block; exit 0 ;;
      --manifest-path)
        [ "$#" -ge 2 ] && [ -n "$2" ] || fail_usage \
          "Missing value for '--manifest-path'." \
          "Pass a path to Cargo.toml, for example: ./scripts/ci/check-features.sh --manifest-path Cargo.toml"
        manifest_path="$2"
        shift 2
        ;;
      *)
        fail_usage "Unknown argument '$1'." \
          "Run './scripts/ci/check-features.sh --help' to inspect supported options."
        ;;
    esac
  done
}

build_feature_check_rows() {
  local metadata_json="$1" workspace_dir="$2"
  METADATA_JSON="$metadata_json" WORKSPACE_DIR="$workspace_dir" python3 - <<'PY'
import json
import os

metadata = json.loads(os.environ["METADATA_JSON"])
crates_prefix = os.path.realpath(os.environ["WORKSPACE_DIR"]).replace("\\", "/") + "/crates/"
packages = sorted(metadata.get("packages", []), key=lambda package: package.get("name", ""))

for package in packages:
    manifest = os.path.realpath(package.get("manifest_path", "")).replace("\\", "/")
    if not manifest.startswith(crates_prefix) or package.get("publish") == []:
        continue
    name = package["name"]
    features = package.get("features") or {}
    print(f"{name}\tcheck\t-p\t{name}\t--no-default-features\t--all-targets")
    for feature_name in sorted(features.keys()):
        if feature_name.startswith("dep:"):
            continue
        if feature_name == "default" and not features.get("default"):
            continue
        print(f"{name}\tcheck\t-p\t{name}\t--no-default-features\t--features\t{feature_name}\t--all-targets")
    print(f"{name}\tcheck\t-p\t{name}\t--all-features\t--all-targets")
PY
}

run_feature_checks() {
  local workspace_dir="$1" rows_text="$2"
  [ -n "$rows_text" ] || fail_error \
    "No publishable workspace crates were found under '$workspace_dir/crates/'." \
    "Verify Cargo.toml workspace.members includes 'crates/*' and at least one crate is publishable."

  local command_count=0 crate_count=0 previous_crate="" row_line crate_name
  while IFS= read -r row_line; do
    [ -n "$row_line" ] || continue
    crate_name="${row_line%%$'\t'*}"
    local args_part="${row_line#*$'\t'}"
    local cargo_args=()
    IFS=$'\t' read -r -a cargo_args <<< "$args_part"
    if [ "$crate_name" != "$previous_crate" ]; then
      crate_count=$((crate_count + 1))
      previous_crate="$crate_name"
    fi
    command_count=$((command_count + 1))
    log_info "cargo ${cargo_args[*]}"
    (cd "$workspace_dir" && cargo "${cargo_args[@]}") >&2 || fail_error \
      "Feature check failed: cargo ${cargo_args[*]}" \
      "Run 'cargo ${cargo_args[*]}' locally and fix the feature-gate compile error above."
  done <<< "$rows_text"

  printf '{"crates_checked":%d,"commands_run":%d}\n' "$crate_count" "$command_count"
  log_success "Verified $command_count feature check command(s) across $crate_count crate(s)."
}

parse_arguments "$@"

command -v cargo >/dev/null 2>&1 || fail_error \
  "Could not find 'cargo' in PATH." \
  "Install Rust via rustup and ensure 'cargo' is in PATH, then retry."
command -v python3 >/dev/null 2>&1 || fail_error \
  "Could not find 'python3' in PATH." \
  "Install Python 3 and ensure 'python3' is in PATH, then retry."

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../.." && pwd)"
target_manifest="${manifest_path:-$repository_root/Cargo.toml}"
[ -f "$target_manifest" ] || fail_error \
  "Workspace manifest not found at '$target_manifest'." \
  "Pass '--manifest-path <Cargo.toml>' or run from an intact avandar-tui-kit checkout."

workspace_directory="$(cd "$(dirname "$target_manifest")" && pwd)"
metadata_output="$(cargo metadata --manifest-path "$target_manifest" --no-deps --format-version 1)" \
  || fail_error "Failed to query cargo metadata for '$target_manifest'." \
    "Run 'cargo metadata --no-deps --format-version 1' to inspect workspace errors."

feature_rows="$(build_feature_check_rows "$metadata_output" "$workspace_directory")"
run_feature_checks "$workspace_directory" "$feature_rows"
