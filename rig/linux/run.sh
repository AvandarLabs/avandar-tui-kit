#!/usr/bin/env bash
#
# Linux floor container runner: builds (on cache miss or --rebuild) and runs
# commands inside the Rust 1.88 + Node 22 + Playwright + tmux 3.3 container
# while isolating Linux target/ and rig/xterm/node_modules/ volumes from host
# artifacts.

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
    "  ${COLOR_BOLD}just linux${COLOR_RESET} [--rebuild] <command> [args...]" \
    "  ${COLOR_BOLD}./rig/linux/run.sh${COLOR_RESET} [--rebuild] <command> [args...]" \
    "" \
    "${COLOR_HEADING}Purpose:${COLOR_RESET}" \
    "  Run <command> inside the Linux floor container (Rust 1.88, Node.js 22," \
    "  Playwright Chromium, and tmux 3.3) with isolated Linux build and npm volumes." \
    "" \
    "${COLOR_HEADING}Arguments:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}<command> [args...]${COLOR_RESET}    Command and arguments executed inside /workspace" \
    "" \
    "${COLOR_HEADING}Options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}-h, --help${COLOR_RESET}             Show this help text on stdout and exit 0" \
    "  ${COLOR_ACCENT}--rebuild${COLOR_RESET}              Force a fresh docker build of the Linux floor image" \
    "  ${COLOR_ACCENT}--${COLOR_RESET}                     Stop parsing runner options before <command>" \
    "" \
    "${COLOR_HEADING}Streams:${COLOR_RESET}" \
    "  stdout                 Exact stdout emitted by <command> inside the container" \
    "  stderr                 Image build/volume progress, errors, and recovery hints" \
    "" \
    "${COLOR_HEADING}Examples:${COLOR_RESET}" \
    "  just linux cargo test --workspace --all-features" \
    "  just linux just check" \
    "  just linux ./rig/xterm/run-tmux.sh full capability_probe" \
    "  ./rig/linux/run.sh --rebuild tmux -V"
}

setup_theme 2

log_info() { printf '%s==>%s %s\n' "$COLOR_ACCENT" "$COLOR_RESET" "$1" >&2; }
log_success() { printf '%s✅ %s%s\n' "$COLOR_SUCCESS" "$1" "$COLOR_RESET" >&2; }

fail_error() {
  printf '%s❌ run.sh: %s%s\n' "$COLOR_ERROR" "$1" "$COLOR_RESET" >&2
  printf '  %s⚠️ %s%s\n' "$COLOR_WARN" "$2" "$COLOR_RESET" >&2
  exit "${3:-1}"
}

fail_usage() { print_usage_block >&2; printf '\n' >&2; fail_error "$1" "$2" 2; }

should_rebuild=0
container_command=()

parse_runner_arguments() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -h|--help) setup_theme 1; print_usage_block; exit 0 ;;
      --rebuild) should_rebuild=1; shift ;;
      --) shift; break ;;
      -*)
        fail_usage "Unknown option '$1' before <command>." \
          "Pass '--rebuild', '--help', or place <command> first (for example: just linux cargo test)."
        ;;
      *) break ;;
    esac
  done
  [ "$#" -gt 0 ] || fail_usage "Missing required <command> argument." \
    "Pass a command to run in the container, for example: just linux cargo test --workspace --all-features"
  container_command=("$@")
}

compute_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$@"
  else
    fail_error "Could not find 'sha256sum' or 'shasum' in PATH." \
      "Install coreutils or perl so SHA-256 image tags can be computed."
  fi
}

build_image_tag() {
  local input_digests combined_hash
  input_digests="$(
    cd "$repository_root" && compute_sha256 \
      rig/linux/Dockerfile \
      rig/linux/entrypoint.sh \
      rig/xterm/package.json \
      rig/xterm/package-lock.json
  )"
  combined_hash="$(printf '%s\n' "$input_digests" | compute_sha256 | awk '{print $1}')"
  printf 'avandar-tui-kit-linux:%s\n' "${combined_hash:0:12}"
}

resolve_arch_tag() {
  local host_arch
  host_arch="$(uname -m)"
  case "$host_arch" in
    aarch64|arm64) printf 'arm64\n' ;;
    x86_64|amd64) printf 'amd64\n' ;;
    *) printf '%s\n' "$host_arch" | tr -c 'A-Za-z0-9_.-' '_' ;;
  esac
}

staged_context_dir=""

cleanup_resources() {
  if [ -n "${staged_context_dir:-}" ]; then
    rm -rf "$staged_context_dir"
    staged_context_dir=""
  fi
}

trap cleanup_resources EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

stage_minimal_build_context() {
  local context_dir="$1"
  mkdir -p "$context_dir/rig/linux" "$context_dir/rig/xterm"
  cp -p "$repository_root/rig/linux/Dockerfile" "$context_dir/rig/linux/Dockerfile"
  cp -p "$repository_root/rig/linux/entrypoint.sh" "$context_dir/rig/linux/entrypoint.sh"
  cp -p "$repository_root/rig/xterm/package.json" "$context_dir/rig/xterm/package.json"
  cp -p "$repository_root/rig/xterm/package-lock.json" "$context_dir/rig/xterm/package-lock.json"
}

ensure_linux_image() {
  if [ "$should_rebuild" -eq 0 ] && docker image inspect "$image_tag" >/dev/null 2>&1; then
    return 0
  fi
  log_info "Building Linux floor image '$image_tag'..."
  local scratch_base="${TMPDIR:-/tmp}" build_status=0
  staged_context_dir="$(mktemp -d "${scratch_base%/}/tuikit-linux-ctx.XXXXXX")"
  stage_minimal_build_context "$staged_context_dir"
  docker build -f "$staged_context_dir/rig/linux/Dockerfile" -t "$image_tag" "$staged_context_dir" >&2 \
    || build_status=$?
  cleanup_resources
  [ "$build_status" -eq 0 ] || fail_error "Failed to build Linux floor image '$image_tag'." \
    "Inspect the docker build output above and verify network access and rig/linux/Dockerfile."
  log_success "Built Linux floor image '$image_tag'."
}

ensure_writable_volumes() {
  if [ "$should_rebuild" -eq 0 ] \
    && docker volume inspect "$cargo_target_volume" >/dev/null 2>&1 \
    && docker volume inspect "$cargo_home_volume" >/dev/null 2>&1 \
    && docker volume inspect "$xterm_modules_volume" >/dev/null 2>&1; then
    return 0
  fi
  log_info "Preparing writable Linux cache volumes ($arch_tag)..."
  docker run --rm --entrypoint chmod \
    -v "${cargo_target_volume}:/mnt/cargo-target" \
    -v "${cargo_home_volume}:/mnt/cargo-home" \
    -v "${xterm_modules_volume}:/mnt/xterm-modules" \
    "$image_tag" 0777 /mnt/cargo-target /mnt/cargo-home /mnt/xterm-modules >&2 \
    || fail_error "Failed to initialize Linux cache volumes." \
      "Verify Docker volume health with 'docker volume ls' and retry."
}

resolve_forwarded_log_dir() {
  local host_log_directory="$1"
  if [ "$host_log_directory" = "$repository_root" ]; then
    printf '/workspace\n'
  elif [[ "$host_log_directory" == "$repository_root/"* ]]; then
    printf '/workspace/%s\n' "${host_log_directory#"$repository_root"/}"
  else
    printf '%s\n' "$host_log_directory"
  fi
}

run_in_container() {
  local docker_args=(
    run --rm --init --shm-size=1g
    --user "$(id -u):$(id -g)"
    -v "$repository_root:/workspace"
    -v "${cargo_target_volume}:/var/cache/tuikit/cargo-target"
    -v "${cargo_home_volume}:/var/cache/tuikit/cargo-home"
    -v "${xterm_modules_volume}:/workspace/rig/xterm/node_modules"
    -e "HOME=/tmp/tuikit-home"
    -e "CARGO_TARGET_DIR=/var/cache/tuikit/cargo-target"
    -e "CARGO_HOME=/var/cache/tuikit/cargo-home"
    -e "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright"
  )
  [ -n "${NO_COLOR:-}" ] && docker_args+=(-e "NO_COLOR=${NO_COLOR}")
  if [ -n "${RIG_XTERM_LOG_DIR:-}" ]; then
    docker_args+=(-e "RIG_XTERM_LOG_DIR=$(resolve_forwarded_log_dir "$RIG_XTERM_LOG_DIR")")
  fi
  docker_args+=("$image_tag" "${container_command[@]}")

  log_info "Running in Linux container ($image_tag): ${container_command[*]}"
  if docker "${docker_args[@]}"; then
    log_success "Completed Linux container command."
  else
    local exit_status=$?
    fail_error "Command failed inside Linux container (exit $exit_status)." \
      "Inspect the command output above or re-run with '--rebuild' if container dependencies changed." \
      "$exit_status"
  fi
}

parse_runner_arguments "$@"

command -v docker >/dev/null 2>&1 || fail_error \
  "Could not find 'docker' in PATH." \
  "Install Docker Desktop or Docker Engine and ensure 'docker' is in PATH, then retry."

docker info >/dev/null 2>&1 || fail_error \
  "Docker daemon is not reachable." \
  "Start Docker Desktop or the Docker Engine service, verify 'docker info' succeeds, and retry."

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../.." && pwd)"
[ -f "$repository_root/Cargo.toml" ] || fail_error \
  "Workspace Cargo.toml not found at '$repository_root/Cargo.toml'." \
  "Run this script from an intact avandar-tui-kit checkout."

# Pre-create the nested volume mountpoint as the caller so Docker on Linux never creates it as root:root.
mkdir -p "$repository_root/rig/xterm/node_modules"
arch_tag="$(resolve_arch_tag)"
cargo_target_volume="tuikit-linux-cargo-target-${arch_tag}"
cargo_home_volume="tuikit-linux-cargo-home-${arch_tag}"
xterm_modules_volume="tuikit-linux-xterm-modules-${arch_tag}"
image_tag="$(build_image_tag)"

ensure_linux_image
ensure_writable_volumes
run_in_container
