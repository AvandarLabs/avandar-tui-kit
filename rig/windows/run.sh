#!/usr/bin/env bash
#
# Layer 7 runner (`just windows`): inspects the Windows jobs of a branch's
# latest GitHub Actions `ci.yml` run, optionally waits for completion via
# `gh run watch`, and downloads Windows job logs and event-log artifacts.

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
    "  ${COLOR_BOLD}just windows${COLOR_RESET} [--branch <branch>] [--run-id <id>] [--wait] [--output-dir <dir>]" \
    "  ${COLOR_BOLD}./rig/windows/run.sh${COLOR_RESET} [--branch <branch>] [--run-id <id>] [--wait] [--output-dir <dir>]" \
    "" \
    "${COLOR_HEADING}Purpose:${COLOR_RESET}" \
    "  Inspect the Windows jobs of a branch's latest ci.yml workflow run and download" \
    "  their logs and uploaded windows-event-logs artifacts (Layer 7)." \
    "" \
    "${COLOR_HEADING}Options:${COLOR_RESET}" \
    "  ${COLOR_ACCENT}-h, --help${COLOR_RESET}             Show this help text on stdout and exit 0" \
    "  ${COLOR_ACCENT}--branch <branch>${COLOR_RESET}      Git branch to query (default: current checked-out branch)" \
    "  ${COLOR_ACCENT}--run-id <id>${COLOR_RESET}          Explicit GitHub Actions run database ID to inspect" \
    "  ${COLOR_ACCENT}--wait${COLOR_RESET}                 Wait for an in-progress run via 'gh run watch' before downloading" \
    "  ${COLOR_ACCENT}--output-dir <dir>${COLOR_RESET}     Destination directory (default: target/windows-ci/<run_id>)" \
    "" \
    "${COLOR_HEADING}Streams:${COLOR_RESET}" \
    "  stdout                 Single-line machine-parseable JSON summary of the Windows CI run" \
    "  stderr                 Run/job status summary, download progress, warnings, and hints" \
    "" \
    "${COLOR_HEADING}Examples:${COLOR_RESET}" \
    "  just windows" \
    "  just windows --wait" \
    "  ./rig/windows/run.sh --branch feat/m0-test-rig --output-dir target/windows-ci/latest"
}

setup_theme 2

log_info() { printf '%s==>%s %s\n' "$COLOR_ACCENT" "$COLOR_RESET" "$1" >&2; }
log_warn() { printf '%s⚠️ %s%s\n' "$COLOR_WARN" "$1" "$COLOR_RESET" >&2; }
log_success() { printf '%s✅ %s%s\n' "$COLOR_SUCCESS" "$1" "$COLOR_RESET" >&2; }

fail_error() {
  printf '%s❌ run.sh: %s%s\n' "$COLOR_ERROR" "$1" "$COLOR_RESET" >&2
  printf '  %s⚠️ %s%s\n' "$COLOR_WARN" "$2" "$COLOR_RESET" >&2
  exit "${3:-1}"
}

fail_usage() { print_usage_block >&2; printf '\n' >&2; fail_error "$1" "$2" 2; }

branch_name=""
run_id=""
should_wait=0
output_dir=""

parse_arguments() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -h|--help) setup_theme 1; print_usage_block; exit 0 ;;
      --branch)
        [ "$#" -ge 2 ] && [ -n "$2" ] || fail_usage "Missing value for '--branch'." \
          "Pass a branch name, for example: just windows --branch main"
        branch_name="$2"; shift 2 ;;
      --run-id)
        [ "$#" -ge 2 ] && [[ "$2" =~ ^[0-9]+$ ]] || fail_usage \
          "Invalid or missing numeric '--run-id' value." \
          "Pass a numeric GitHub Actions database ID, for example: just windows --run-id 123456789"
        run_id="$2"; shift 2 ;;
      --wait) should_wait=1; shift ;;
      --output-dir)
        [ "$#" -ge 2 ] && [ -n "$2" ] || fail_usage "Missing value for '--output-dir'." \
          "Pass a directory path, for example: just windows --output-dir target/windows-ci/custom"
        output_dir="$2"; shift 2 ;;
      *)
        fail_usage "Unknown argument '$1'." \
          "Run './rig/windows/run.sh --help' to inspect supported options." ;;
    esac
  done
}

check_prerequisites() {
  command -v gh >/dev/null 2>&1 || fail_error \
    "Could not find 'gh' in PATH." \
    "Install GitHub CLI (https://cli.github.com), run 'gh auth login', and retry."
  command -v python3 >/dev/null 2>&1 || fail_error \
    "Could not find 'python3' in PATH." \
    "Install Python 3 and ensure 'python3' is in PATH, then retry."
  gh auth status >/dev/null 2>&1 || fail_error \
    "GitHub CLI is not authenticated." \
    "Run 'gh auth status' and 'gh auth login', then retry."
}

resolve_target_run_id() {
  if [ -n "$run_id" ]; then
    return 0
  fi
  if [ -z "$branch_name" ]; then
    command -v git >/dev/null 2>&1 || fail_error "Could not find 'git' in PATH." \
      "Install git or pass '--branch <branch>' or '--run-id <id>'."
    branch_name="$(git -C "$repository_root" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
    [ -n "$branch_name" ] || fail_error \
      "Repository HEAD is detached and neither '--branch' nor '--run-id' was supplied." \
      "Check out a branch or pass '--branch <branch>' or '--run-id <id>'."
  fi
  log_info "Looking up latest ci.yml workflow run for branch '$branch_name'..."
  local list_json
  list_json="$(cd "$repository_root" && gh run list \
    --branch "$branch_name" --workflow ci.yml --limit 1 \
    --json databaseId,status,conclusion,headSha,url,workflowName,createdAt)" \
    || fail_error "Failed to query ci.yml runs for branch '$branch_name'." \
      "Verify the repository has a GitHub remote and Actions access via 'gh run list'."
  run_id="$(LIST_JSON="$list_json" python3 -c '
import json, os
items = json.loads(os.environ["LIST_JSON"])
print(items[0]["databaseId"] if items else "")
')"
  [ -n "$run_id" ] || fail_error \
    "No ci.yml workflow runs found for branch '$branch_name'." \
    "Push '$branch_name' to origin ('git push -u origin $branch_name') and retry with 'just windows --wait'."
}

downloaded_map_file=""

cleanup_temp_files() {
  if [ -n "${downloaded_map_file:-}" ]; then
    rm -f "$downloaded_map_file"
    downloaded_map_file=""
  fi
}

trap cleanup_temp_files EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

query_run_view_json() {
  (cd "$repository_root" && gh run view "$run_id" \
    --json databaseId,headBranch,status,conclusion,headSha,url,jobs) \
    || fail_error "Failed to inspect workflow run '$run_id'." \
      "Verify run ID '$run_id' exists with 'gh run view $run_id'."
}

fetch_and_optionally_watch_run() {
  run_view_json="$(query_run_view_json)"
  local current_status watch_status=0
  current_status="$(RUN_JSON="$run_view_json" python3 -c \
    'import json, os; print(json.loads(os.environ["RUN_JSON"]).get("status") or "")')"
  if [ "$should_wait" -eq 1 ] && [ "$current_status" != "completed" ]; then
    log_info "Waiting for workflow run $run_id to complete..."
    (cd "$repository_root" && gh run watch "$run_id" >&2) || watch_status=$?
    if [ "$watch_status" -eq 130 ] || [ "$watch_status" -eq 143 ]; then
      exit "$watch_status"
    fi
    run_view_json="$(query_run_view_json)"
  fi
}

extract_windows_job_rows() {
  RUN_JSON="$run_view_json" python3 - <<'PY'
import json, os, re
data = json.loads(os.environ["RUN_JSON"])
for job in data.get("jobs") or []:
    name = job.get("name") or ""
    if "windows" not in name.lower():
        continue
    job_id = str(job.get("databaseId") or "")
    safe_name = re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("_") or "Windows"
    status = job.get("status") or "unknown"
    conclusion = job.get("conclusion") or ""
    print(f"{job_id}\t{safe_name}\t{status}\t{conclusion}\t{name}")
PY
}

prepare_output_directory() {
  if [ -z "$output_dir" ]; then
    output_dir="$repository_root/target/windows-ci/$run_id"
  elif [[ "$output_dir" != /* ]]; then
    output_dir="$caller_cwd/$output_dir"
  fi
  mkdir -p "$output_dir" || fail_error \
    "Could not create output directory '$output_dir'." \
    "Verify the parent directory of '$output_dir' is writable and retry."
  output_dir="$(cd "$output_dir" && pwd)"
}

download_logs_and_artifacts() {
  local job_rows="$1"
  rm -rf "$output_dir/artifacts"
  mkdir -p "$output_dir/jobs" "$output_dir/artifacts"
  downloaded_map_file="$output_dir/jobs/.downloaded.tsv"
  : > "$downloaded_map_file"
  local job_id safe_name job_status job_conclusion raw_name
  while IFS=$'\t' read -r job_id safe_name job_status job_conclusion raw_name; do
    [ -n "$job_id" ] || continue
    log_info "Windows job '$raw_name' (#$job_id): status=$job_status conclusion=${job_conclusion:-pending}"
    if [ "$job_status" = "completed" ]; then
      local log_file="$output_dir/jobs/${safe_name}-${job_id}.log"
      if (cd "$repository_root" && gh run view "$run_id" --job "$job_id" --log) > "$log_file"; then
        printf '%s\t%s\n' "$job_id" "$log_file" >> "$downloaded_map_file"
      else
        rm -f "$log_file"
        log_warn "Could not download job log for '$raw_name' (#$job_id)."
      fi
    else
      log_warn "Job '$raw_name' is '$job_status'; pass '--wait' to wait before downloading its log."
    fi
  done <<< "$job_rows"

  if ! (cd "$repository_root" && gh run download "$run_id" \
    --pattern "windows-event-logs*" --dir "$output_dir/artifacts" >&2); then
    log_warn "No 'windows-event-logs*' artifacts were downloaded for run $run_id."
  fi
}

build_summary_state() {
  RUN_JSON="$run_view_json" BRANCH_HINT="$branch_name" OUT_DIR="$output_dir" \
    MAP_FILE="$downloaded_map_file" python3 - <<'PY'
import json, os

data = json.loads(os.environ["RUN_JSON"])
log_map = {}
with open(os.environ["MAP_FILE"], "r", encoding="utf-8") as handle:
    for line in handle:
        parts = line.rstrip("\n").split("\t", 1)
        if len(parts) == 2:
            log_map[parts[0]] = parts[1]

windows_jobs = [
    {
        "id": job.get("databaseId"),
        "name": job.get("name") or "",
        "status": job.get("status"),
        "conclusion": job.get("conclusion"),
        "log_path": log_map.get(str(job.get("databaseId"))),
    }
    for job in (data.get("jobs") or [])
    if "windows" in (job.get("name") or "").lower()
]
summary = {
    "run_id": data.get("databaseId"),
    "branch": os.environ["BRANCH_HINT"] or data.get("headBranch"),
    "head_sha": data.get("headSha"),
    "status": data.get("status"),
    "conclusion": data.get("conclusion"),
    "url": data.get("url"),
    "output_dir": os.environ["OUT_DIR"],
    "windows_jobs": windows_jobs,
}
is_all_ok = bool(windows_jobs) and all(
    item["status"] == "completed" and item["conclusion"] == "success" for item in windows_jobs
)
print(json.dumps(summary, separators=(",", ":")))
print("OK" if is_all_ok else "FAIL")
PY
}

emit_summary_and_finish() {
  local summary_state
  summary_state="$(build_summary_state)"
  cleanup_temp_files
  local json_line="${summary_state%%$'\n'*}"
  local outcome_token="${summary_state##*$'\n'}"
  printf '%s\n' "$json_line"
  if [ "$outcome_token" = "OK" ]; then
    log_success "All Windows CI jobs succeeded for run $run_id (logs saved in '$output_dir')."
    exit 0
  fi
  fail_error "Windows CI jobs did not succeed for run $run_id." \
    "Inspect downloaded logs under '$output_dir' (or re-run with '--wait' if still in progress)." 1
}

caller_cwd="$(pwd)"
parse_arguments "$@"
check_prerequisites

script_directory="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$script_directory/../.." && pwd)"
run_view_json=""

resolve_target_run_id
fetch_and_optionally_watch_run

windows_rows="$(extract_windows_job_rows)"
[ -n "$windows_rows" ] || fail_error \
  "Workflow run $run_id contains no Windows jobs." \
  "Verify '.github/workflows/ci.yml' defines a job whose name includes 'Windows'."

prepare_output_directory
download_logs_and_artifacts "$windows_rows"
emit_summary_and_finish
