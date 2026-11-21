# Milestone 0 Part 4 (`rig/iterm2`, `rig/expected`, and Documentation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Layer 6 (`rig/iterm2` Tart + VNC driver + `just iterm2`), measure all iTerm2, `tmux`-in-iTerm2, and `tmux -CC` rows (M0 rows 1, 4, 5, 6, 7, 9, 10, 11, 12, 13, 19, 20), pass 10 consecutive cold-start runs of the `tuikit-macos` VM, commit all 13 `rig/expected` baselines with automated verification, and update `docs/terminals.md`, `docs/testing.md`, `docs/architecture.md`, `docs/decisions.md`, and `README.md` so every Milestone 0 exit criterion is satisfied.

**Architecture:** `rig/iterm2` is a modular Python 3 + shell driver (`rig/iterm2/run.sh` + `rig/iterm2/driver/*.py`) that boots the prepared `tuikit-macos` Tart VM headless with VNC and a VirtioFS shared directory (`--dir`), waits on condition-based guest readiness via `tart exec` and VNC port readiness (zero fixed sleeps), calibrates VNC key mapping for macOS `Option` before measuring key chords, launches iTerm2 sessions at controlled cell/pixel geometry, injects real hardware pointer and keyboard events through `vncdo`, captures a VNC screenshot automatically on failure, and stops the VM cleanly on exit. `rig/expected` stores the normalized machine-readable capability baselines for all 13 M0 configurations and checks live probe runs against them in `cargo test` and CI.

**Tech Stack:** Tart (`2.42+`, `tuikit-macos` macOS 15 guest with iTerm2 and `tmux`), `vncdotool` (`vncdo`), Python 3 (`unittest` stdlib), Bash 3.2+, Node.js (`>= 20`), Rust 1.88 (`edition = "2024"`).

---

## Global Constraints

- Zero em dashes anywhere in code, comments, Python, JS, Rust, shell scripts, JSON, Markdown docs, or commit messages.
- Zero `sleep` calls for synchronization anywhere in the rig: wait on guest agent readiness (`tart exec`), TCP/RFB port handshake, `<log>.ready` sentinels, FIFO key fences, and process exit, with bounded timeouts that only ever mean failure.
- Every Rust, Python, JavaScript, shell, and test file `<= 400` lines, and every function (production and test across Rust, Python, JS, and shell) `<= 45` lines.
- All CLI entry points (`rig/iterm2/run.sh`) follow `cli-command-conventions`: `-h`/`--help` first to `stdout`, machine JSON on `stdout`, progress/`✅`/`❌` + indented `  ⚠️` recovery hints on `stderr`, and `NO_COLOR` / `TERM=dumb` / non-TTY plain mode support.
- Public `README.md` must never name private/personal source apps (`brain`, `huckleberry-cli`).
- Delete this plan file from `docs/plans/` as soon as Part 4 implementation and its final whole-part review complete (`AGENTS.md`).

---

### Task 1: Build Layer 6 (`rig/iterm2` Tart + VNC Driver + `just iterm2`), Measure All iTerm2 & `tmux` / `tmux -CC` Rows, and Pass 10 Cold-Start Runs

**Files:**
- Create: `rig/iterm2/run.sh` (`chmod +x`)
- Create: `rig/iterm2/driver/__init__.py`
- Create: `rig/iterm2/driver/cli_args.py`
- Create: `rig/iterm2/driver/tart_vm.py`
- Create: `rig/iterm2/driver/vnc_input.py`
- Create: `rig/iterm2/driver/iterm_window.py`
- Create: `rig/iterm2/driver/scenarios.py`
- Create: `rig/iterm2/driver/matrix_probes.py`
- Create: `rig/iterm2/driver/main.py`
- Create: `rig/iterm2/tests/test_driver_unit.py`
- Create: `rig/probe/tests/iterm2_runner_cli.rs`
- Modify: `justfile` (add `iterm2 *args:` -> `./rig/iterm2/run.sh "$@"`)

**Interfaces:**
- `just iterm2 <program> [scenario] [--tmux <stock|full|cc-stock|cc-full>] [extra_args...]`:
  - Scenarios: `all` (default for `capability_probe`), `batch`, `osc52`, `interactive`, `panic`, `matrix` (runs the full iTerm2 M0 matrix sweeps: natural scrolling off/on for Row 1, selection drag modifiers + guest `pbpaste` for Row 4, Option default vs `Esc+` for Row 5, Kitty Enter modifiers for Row 6, `AllowClipboardAccess` off/on for Row 7, `stock`/`full` tmux for Rows 9 to 12, and `tmux -CC` with `stock` and `full` for Row 13, always restoring guest `defaults` in `finally`), and `passthrough`.
  - Captures a VNC screenshot (`failure-screenshot.png`) automatically if any step fails, and always stops `tuikit-macos` on exit (`atexit` + `SIGINT`/`SIGTERM` + `finally`).

- [ ] **Step 1: Inspect `tart run --help` and `vncdo --help`, then write failing Python unit tests (`rig/iterm2/tests/test_driver_unit.py`) and Rust CLI contract tests (`rig/probe/tests/iterm2_runner_cli.rs`)**
- [ ] **Step 2: Run `python3 -m unittest discover -s rig/iterm2/tests` and `cargo test -p rig-probe --test iterm2_runner_cli` and verify failure (Red)**
- [ ] **Step 3: Implement `rig/iterm2/driver/*.py`, `rig/iterm2/run.sh`, and `justfile` `iterm2` recipe**
- [ ] **Step 4: Run unit & CLI tests, execute `just iterm2 capability_probe` (`all` + `matrix` + `--tmux stock` + `--tmux full` + `--tmux cc-stock` + `--tmux cc-full`) to record all M0 rows 1, 4, 5, 6, 7, 9, 10, 11, 12, 13, 19, 20, and execute 10 consecutive cold-start runs of `tuikit-macos`**
- [ ] **Step 5: Run `just check` and `just lint-targets`, then commit (`feat(rig): add Layer 6 iTerm2 Tart+VNC driver and verify 10 cold-start runs`)**

---

### Task 2: Create All 13 `rig/expected` Baselines and Automated Baseline Verification

**Files:**
- Create: `rig/expected/xterm-macos.json`
- Create: `rig/expected/xterm-linux.json`
- Create: `rig/expected/xterm-windows.json`
- Create: `rig/expected/xterm-tmux-stock-macos.json`
- Create: `rig/expected/xterm-tmux-full-macos.json`
- Create: `rig/expected/xterm-tmux-stock-linux.json`
- Create: `rig/expected/xterm-tmux-full-linux.json`
- Create: `rig/expected/iterm2.json`
- Create: `rig/expected/iterm2-tmux-stock.json`
- Create: `rig/expected/iterm2-tmux-full.json`
- Create: `rig/expected/iterm2-tmux-cc.json`
- Create: `rig/expected/orca.json`
- Create: `rig/expected/orca-tmux.json`
- Create: `rig/xterm/src/expected_compare.mjs`
- Create: `rig/xterm/test/expected.test.mjs` (and register in `rig/xterm/package.json`)
- Create: `rig/probe/tests/expected_baselines.rs`
- Modify: `.github/workflows/ci.yml` (verify live CI probe output matches `rig/expected` on Linux, macOS, and Windows)

**Interfaces:**
- Each `rig/expected/<config>.json` captures the canonical, timing-independent capability baseline for that configuration (`identity`, `mouse_modes`, `kitty_keyboard`, `modes`, `background_color` `answered`/`is_dark`, `cell_size` `answered`, all 33 `widths` samples, `osc52` outcomes, and interactive pointer/keyboard/paste/focus/teardown capabilities where applicable).
- `expected_compare.mjs` normalizes live probe reports into the `rig/expected` schema and asserts exact structural parity (`compareReportAgainstExpected(liveReport, expectedBaseline)`), verified against all 13 files and wired into CI for the 7 CI-hosted configurations (`xterm-{macos,linux,windows}`, `xterm-tmux-{stock,full}-{macos,linux}`) and verified locally against `orca`, `orca-tmux`, and all 4 `iterm2*` configurations.

- [ ] **Step 1: Write failing Rust & Node tests (`rig/probe/tests/expected_baselines.rs` and `rig/xterm/test/expected.test.mjs`) asserting all 13 baseline files exist, validate against schema, contain zero em dashes, and match normalized probe outputs**
- [ ] **Step 2: Run `cargo test -p rig-probe --test expected_baselines` and `npm --prefix rig/xterm test` and verify failure (Red)**
- [ ] **Step 3: Populate all 13 `rig/expected/*.json` files from the real measured probe runs across Layers 3, 4, 5, 6, and 7, implement `expected_compare.mjs`, and wire baseline verification into `.github/workflows/ci.yml`**
- [ ] **Step 4: Verify live comparison across macOS (`xterm`, `xterm-tmux stock/full`, `orca`, `orca --tmux`), Linux container (`xterm`, `xterm-tmux stock/full`), Windows artifact (`latest-windows-xterm.json`), and iTerm2 (`iterm2`, `iterm2-tmux-stock`, `iterm2-tmux-full`, `iterm2-tmux-cc`), plus `just check && just lint-targets`**
- [ ] **Step 5: Commit (`feat(rig): add all 13 rig/expected capability baselines and CI verification`)**

---

### Task 3: Update Milestone 0 Documentation (`terminals.md`, `testing.md`, `architecture.md`, `decisions.md`, `README.md`), Verify Live CI Green, and Close Milestone 0

**Files:**
- Modify: `docs/terminals.md`
- Modify: `docs/testing.md`
- Modify: `docs/architecture.md`
- Modify: `docs/decisions.md`
- Modify: `README.md`
- Create: `rig/probe/tests/m0_docs_invariants.rs` (verifies zero `"Verify"` left in `docs/terminals.md`, all 13 `rig/expected` files present, all 8 layer recipes present in `justfile`, zero `sleep` across `rig/`, zero em dashes in touched docs, and no private app names in `README.md`)

- [ ] **Step 1: Write failing doc-invariant test in `rig/probe/tests/m0_docs_invariants.rs`**
- [ ] **Step 2: Run `cargo test -p rig-probe --test m0_docs_invariants` and verify failure (Red)**
- [ ] **Step 3: Update `docs/terminals.md`, `docs/testing.md`, `docs/architecture.md`, `docs/decisions.md`, and `README.md` with all empirical Milestone 0 measurements and layer documentation**
- [ ] **Step 4: Run `just check`, `just lint-targets`, `just linux just check`, and `npm --prefix rig/xterm test`, commit (`docs(m0): record all terminal measurements and complete Milestone 0 docs`), push `feat/m0-test-rig`, and verify CI is 100% green across `Linux`, `macOS`, and `Windows` (`just windows --wait`)**
