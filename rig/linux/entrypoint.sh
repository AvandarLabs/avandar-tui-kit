#!/usr/bin/env bash
#
# Container entrypoint for rig/linux: prepares writable runtime directories
# and populates the isolated Linux node_modules volume before exec-ing "$@"
# inside /workspace.

set -euo pipefail

prepare_runtime_directories() {
  export HOME="${HOME:-/tmp/tuikit-home}"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/var/cache/tuikit/cargo-target}"
  export CARGO_HOME="${CARGO_HOME:-/var/cache/tuikit/cargo-home}"
  mkdir -p "$HOME" "$CARGO_TARGET_DIR" "$CARGO_HOME"
}

sync_xterm_node_modules() {
  local xterm_dir="/workspace/rig/xterm"
  local lockfile_path="$xterm_dir/package-lock.json"
  local target_modules="$xterm_dir/node_modules"
  local target_stamp="$target_modules/.linux-lock.sha256"
  local image_modules="/opt/tuikit-xterm-deps/node_modules"
  local image_stamp="/opt/tuikit-xterm-deps/.linux-lock.sha256"

  [ -f "$lockfile_path" ] || return 0
  mkdir -p "$target_modules"

  local desired_hash current_hash="" image_hash=""
  desired_hash="$(sha256sum "$lockfile_path" | awk '{print $1}')"
  [ -f "$target_stamp" ] && current_hash="$(tr -d '[:space:]' < "$target_stamp")"
  [ "$current_hash" = "$desired_hash" ] && return 0

  rm -f "$target_stamp"
  find "$target_modules" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  [ -f "$image_stamp" ] && image_hash="$(tr -d '[:space:]' < "$image_stamp")"
  if [ "$image_hash" = "$desired_hash" ] && [ -d "$image_modules" ]; then
    cp -RP "$image_modules/." "$target_modules/"
  else
    npm --prefix "$xterm_dir" ci >&2
  fi
  printf '%s\n' "$desired_hash" > "$target_stamp"
}

prepare_runtime_directories
sync_xterm_node_modules
exec "$@"
