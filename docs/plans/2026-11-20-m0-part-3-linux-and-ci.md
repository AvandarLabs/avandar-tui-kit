# Milestone 0 Part 3 (`rig/linux`, CI, and `just windows`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Layer 1/3/4 Linux floor container (`rig/linux` + `just linux` with Rust 1.88, Node, Playwright Chromium, and `tmux` 3.3), set up GitHub Actions CI across macOS, Linux, and Windows (`.github/workflows/ci.yml`), build Layer 7 (`rig/windows/run.sh` + `just windows`), prove 30 consecutive flake-free runs inside the Linux container, and run live CI on `feat/m0-test-rig` to measure Windows ConPTY rows (M0 rows 14 to 18) and Linux/`tmux` 3.3 rows.

**Architecture:** `rig/linux/Dockerfile` builds on `rust:1.88-bookworm` (which provides Rust 1.88 and Debian Bookworm's `tmux 3.3a` across `linux/arm64` and `linux/amd64`), pre-installs Node.js, `just`, Playwright Chromium (`1.65.1`) + system libraries, and pre-built Linux `rig/xterm/node_modules`. `rig/linux/run.sh` mounts isolated Docker volumes for Linux `CARGO_TARGET_DIR` and `/workspace/rig/xterm/node_modules` so macOS host Mach-O binaries and Linux ELF binaries never collide, and passes `--init --shm-size=1g` for stable headless Chromium execution. `.github/workflows/ci.yml` runs all M0 CI matrix jobs on `push` and `pull_request`, reusing `rig/linux` for every Linux job and uploading event logs (`on failure` for macOS/Linux, `always` for Windows). `rig/windows/run.sh` queries `gh` for the current branch's latest CI run, reports Windows job statuses, and downloads Windows job logs and event-log artifacts.

**Tech Stack:** Docker (`rust:1.88-bookworm`, `tmux 3.3a`, Node.js 22 LTS, Playwright `1.65.1`), Bash 3.2+, GitHub Actions, GitHub CLI (`gh`), Rust 1.88 (`edition = "2024"`).

---

## Global Constraints

- Zero em dashes anywhere in code, comments, Dockerfiles, YAML workflows, help text, docs, or commit messages.
- Zero `sleep` calls for synchronization: wait on conditions (`gh run watch`, WebSocket readiness, `.ready` sentinels, `DA1` / `F2` fences, process exit).
- Every Rust, JavaScript, shell, Dockerfile, YAML, and test file `<= 400` lines, and every function (Rust, JS, Python, and shell, production and test) `<= 45` lines.
- All CLI entry points (`rig/linux/run.sh`, `rig/windows/run.sh`, `scripts/ci/check-features.sh`) follow `cli-command-conventions`: `-h`/`--help` first to `stdout`, machine output on `stdout`, progress/`✅`/`❌` + indented `  ⚠️` recovery hints on `stderr`, and `NO_COLOR` / `TERM=dumb` / non-TTY plain mode support.
- Delete this plan file from `docs/plans/` as soon as Part 3 implementation and its final whole-part review complete (`AGENTS.md`).

---

### Task 1: Build `rig/linux` Container (`Dockerfile` + `rig/linux/run.sh` + `just linux`), Measure Linux & `tmux 3.3`, and Pass 30 Consecutive Container Runs

**Files:**
- Create: `rig/linux/Dockerfile`
- Create: `rig/linux/entrypoint.sh` (`chmod +x`)
- Create: `rig/linux/run.sh` (`chmod +x`)
- Modify: `justfile` (add `linux *args:` -> `./rig/linux/run.sh "$@"`)
- Create: `rig/probe/tests/linux_runner_cli.rs`

**Interfaces:**
- `rig/linux/Dockerfile`:
  - Base `rust:1.88-bookworm` (`rustfmt` + `clippy` installed; build-time check asserts `rustc -V` starts with `rustc 1.88.` and `tmux -V` matches `^tmux 3\.3`).
  - Installs Node.js 22 LTS (`linux-arm64` / `linux-x64` matched via `dpkg --print-architecture`), `just`, `tmux`, `python3`, `make`, `g++`, `pkg-config`, fonts (`fonts-liberation`, `fonts-dejavu-core`, `fonts-noto-color-emoji`, `fonts-noto-cjk`), and Playwright Chromium (`PLAYWRIGHT_BROWSERS_PATH=/ms-playwright npx playwright@1.65.1 install --with-deps chromium`, world-readable).
  - Pre-builds `/opt/tuikit-xterm-deps/node_modules` from `rig/xterm/package.json` + `rig/xterm/package-lock.json`.
- `rig/linux/entrypoint.sh` + `rig/linux/run.sh`:
  - `just linux <command> [args...]` builds/tags the image keyed by a hash of `rig/linux/Dockerfile`, `rig/linux/entrypoint.sh`, and `rig/xterm/package-lock.json` (logging build output to `stderr` only), mounts `/workspace`, mounts isolated named volumes for Linux `CARGO_TARGET_DIR` and `/workspace/rig/xterm/node_modules` (seeded from `/opt/tuikit-xterm-deps/node_modules`), runs with `--rm --init --shm-size=1g`, and forwards `stdout` cleanly.

- [ ] **Step 1: Write failing Rust CLI contract tests in `rig/probe/tests/linux_runner_cli.rs` (`--help`, invalid args, missing/mock `docker` invocation, `NO_COLOR`, zero `sleep`/em-dash checks)**
- [ ] **Step 2: Run `cargo test -p rig-probe --test linux_runner_cli` and verify failure (Red)**
- [ ] **Step 3: Implement `rig/linux/Dockerfile`, `rig/linux/entrypoint.sh`, `rig/linux/run.sh`, and `justfile` `linux` recipe**
- [ ] **Step 4: Run `cargo test -p rig-probe --test linux_runner_cli`, build the container via `just linux`, verify `just linux just check`, `just linux npm --prefix rig/xterm test`, `just linux ./rig/xterm/run.sh capability_probe`, and `just linux ./rig/xterm/run-tmux.sh {stock,full} capability_probe` (recording Linux & `tmux 3.3a` measurements), and execute 30 consecutive container runs for Layer 3 and Layer 4 (`stock` and `full`)**
- [ ] **Step 5: Run `just check` and `just lint-targets`, then commit (`feat(rig): add rig/linux floor container with Rust 1.88 and tmux 3.3`)**

---

### Task 2: Add `RIG_XTERM_LOG_DIR` Artifact Archiving, `scripts/ci/check-features.sh`, `rig/windows/run.sh` (`just windows`), and `.github/workflows/ci.yml`

**Files:**
- Modify: `rig/xterm/src/scenarios.mjs` & `rig/xterm/src/harness.mjs` (archive `probe.jsonl` and report JSON into `RIG_XTERM_LOG_DIR` when set, including on failure)
- Modify: `rig/xterm/test/scenarios.test.mjs` or create `rig/xterm/test/log_archive.test.mjs` + update `rig/xterm/package.json` test list
- Create: `scripts/ci/check-features.sh` (`chmod +x`)
- Create: `rig/windows/run.sh` (`chmod +x`)
- Modify: `justfile` (add `windows *args:` -> `./rig/windows/run.sh "$@"`)
- Create: `rig/probe/tests/windows_runner_cli.rs`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- `RIG_XTERM_LOG_DIR`: when set to a directory path, `scenarios.mjs` copies every scenario/sub-run `probe.jsonl` (even when an assertion fails before cleanup) into `RIG_XTERM_LOG_DIR`, and `harness.mjs` writes the structured scenario report JSON into `RIG_XTERM_LOG_DIR`.
- `scripts/ci/check-features.sh`: enumerates workspace library/binary crates under `crates/*` via `cargo metadata` and runs one `cargo check` command per combination (`--no-default-features`, each feature alone, and `--all-features`).
- `just windows` (`./rig/windows/run.sh`): supports `-h`/`--help`, `--branch <name>`, `--run-id <id>`, `--wait`, and `--output-dir <dir>`; displays Windows CI job statuses and downloads Windows job logs and `windows-event-logs` artifacts via `gh`.
- `.github/workflows/ci.yml`: triggers on `push` and `pull_request` across macOS, Linux (`rig/linux` image), and Windows covering every row of the M0 CI table in `docs/milestones.md`:
  - Format check (`Linux`)
  - Clippy, tests, docs (`macOS`, `Linux`, `Windows`)
  - Build and test at Rust 1.88 with every feature (`macOS`, `Linux`, `Windows`)
  - Feature checks, one command per row (`Linux`)
  - `xterm.js` harness (`macOS`, `Linux`, `Windows`)
  - `xterm.js` harness in `stock` and `full` `tmux` (`macOS`, `Linux`)
  - Event logs uploaded (`On failure` on macOS and Linux, `Always` on Windows)

- [ ] **Step 1: Write failing tests for `RIG_XTERM_LOG_DIR` archiving, `scripts/ci/check-features.sh`, and `rig/windows/run.sh` (`windows_runner_cli.rs` with mock `gh`)**
- [ ] **Step 2: Run `npm --prefix rig/xterm test` and `cargo test -p rig-probe --test windows_runner_cli` and verify failure (Red)**
- [ ] **Step 3: Implement `RIG_XTERM_LOG_DIR` archiving, `scripts/ci/check-features.sh`, `rig/windows/run.sh`, `justfile` `windows` recipe, and `.github/workflows/ci.yml`**
- [ ] **Step 4: Run `npm --prefix rig/xterm test`, `./scripts/ci/check-features.sh`, `just linux ./scripts/ci/check-features.sh`, `just check`, and `just lint-targets`**
- [ ] **Step 5: Commit (`feat(ci): add multi-platform CI workflow, feature checks, event log archiving, and just windows`)**

---

### Task 3: Push `feat/m0-test-rig`, Verify Live Multi-OS CI (macOS, Linux, Windows), and Capture Windows ConPTY Measurements (M0 Rows 14 to 18)

**Files:**
- Any targeted fixes in `rig/` or `.github/workflows/ci.yml` if live CI or Windows `ConPTY` exposes platform quirks.

- [ ] **Step 1: Push `feat/m0-test-rig` to `origin` (`git push -u origin feat/m0-test-rig`)**
- [ ] **Step 2: Monitor the GitHub Actions run (`just windows --wait` / `gh run watch`) until all macOS, Linux, and Windows jobs complete**
- [ ] **Step 3: Run `just windows` to inspect Windows job logs and download `windows-event-logs`, fixing and re-pushing if any CI job fails until every macOS, Linux, and Windows job is green**
- [ ] **Step 4: Extract and record all Windows `ConPTY` measurements from `windows-event-logs` and Windows test logs (M0 rows 14, 15, 16, 17, 18, 19, 20) for Part 4's `rig/expected` and `docs/terminals.md`**
