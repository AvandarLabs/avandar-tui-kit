# Milestone 0 Part 2 (`rig/xterm` and `rig/tmux`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Layer 3 (`rig/xterm` + `just xterm`) and Layer 4 (`rig/tmux` + `just xterm-tmux`), extend Layer 5 (`just orca --tmux`), measure M0 rows 3 (macOS baseline), 9, 10, and 12 on macOS, and prove 30 consecutive flake-free runs.

**Architecture:** `rig/xterm` runs a local HTTP + WebSocket bridge that opens the real browser build of `@xterm/xterm@6.0.0` with `@xterm/addon-unicode11@0.9.0` inside headless Playwright Chromium first, waits until the browser terminal reports ready with measured cell pixel dimensions, and only then attaches `node-pty` and spawns the target binary (Rule 5 of `docs/testing.md`). `rig/tmux` provides `stock.conf` (empty) and `full.conf` (`mouse`, `set-clipboard`, `allow-passthrough`, `extended-keys`, `focus-events`) executed inside isolated private `tmux -L` sockets cleaned up on exit.

**Tech Stack:** Rust 1.88 (`edition = "2024"`), Node.js (`>= 20`, built-in `node:test`), `@xterm/xterm@6.0.0`, `@xterm/addon-unicode11@0.9.0`, `playwright`, `node-pty`, `tmux` (`>= 3.3`), Bash 3.2+.

---

## Global Constraints

- Zero em dashes anywhere in code, comments, help text, docs, or commit messages.
- Zero `sleep` calls for synchronization: wait on WebSocket readiness, `<log>.ready` sentinel files, `DA1` fences, and process exit events, with timeouts that only ever mean failure.
- Every Rust production file `<= 400` lines, every JavaScript/shell file `<= 400` lines, and every function (Rust, JS, and shell, production and test) `<= 45` lines.
- All CLI entry points (`rig/xterm/run.sh`, `rig/xterm/run-tmux.sh`, `rig/orca/run.sh`) follow `cli-command-conventions`: `-h`/`--help` first to `stdout`, machine JSON/JSONL only on `stdout`, progress/`✅`/`❌` + indented `  ⚠️` recovery hints on `stderr`, and `NO_COLOR` / `TERM=dumb` / non-TTY plain mode support.
- Delete this plan file from `docs/plans/` as soon as Part 2 implementation and its final whole-part review complete (`AGENTS.md`).

---

### Task 1: Align `rig/probe` Windows Interactive Modes & `--osc52-form`, Add `rig/tmux` Configs, and Add `--tmux` to `rig/orca/run.sh`

**Files:**
- Modify: `rig/probe/src/probe_args.rs`
- Modify: `rig/probe/src/session_guard.rs`
- Modify: `rig/probe/src/probe_sections.rs`
- Modify: `rig/probe/src/probe_runner.rs`
- Create: `rig/tmux/stock.conf`
- Create: `rig/tmux/full.conf`
- Modify: `rig/orca/run.sh`
- Modify: `rig/probe/tests/orca_runner_cli.rs`

**Interfaces:**
- `rig_probe::probe_args`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Osc52Form { Auto, Plain, Tmux, Both }`
  - `ProbeArgs` adds `pub osc52_form: Osc52Form` (`--osc52-form auto|plain|tmux|both`, default `Osc52Form::Auto`: emits `plain` outside tmux and `both` inside tmux).
- `rig_probe::session_guard`:
  - `TerminalSessionGuard::enable_interactive_modes(&self, writer: &mut TerminalWriter, is_push_kitty: bool, log: &mut JsonLog) -> std::io::Result<()>`
    - `#[cfg(unix)]`: writes `MOUSE_SETUP_M1`, `BRACKETED_PASTE_ON`, `FOCUS_EVENTS_ON`, and (if `is_push_kitty`) `KITTY_KEYBOARD_PUSH_DISAMBIGUATE`.
    - `#[cfg(windows)]`: enables console mouse input via `crossterm::event::EnableMouseCapture` and leaves modes `2004`, `1004`, and Kitty flags off per `docs/specs/architecture.md` Platform Policy.
- `rig/tmux/stock.conf`: 0-byte empty file (`docs/milestones.md`).
- `rig/tmux/full.conf`: sets `mouse on`, `set-clipboard on`, `allow-passthrough on`, `extended-keys on`, `focus-events on`.
- `rig/orca/run.sh`: supports optional `--tmux <stock|full>`, wrapping the probe inside `tmux -L <unique_socket> -f <conf> new-session ...` and killing `<unique_socket>` in the `EXIT` trap.

- [ ] **Step 1: Write failing Rust unit & CLI tests for `--osc52-form`, `enable_interactive_modes`, `rig/tmux/*.conf`, and `rig/orca/run.sh --tmux`**
- [ ] **Step 2: Run `cargo test -p rig-probe` and verify failure (Red)**
- [ ] **Step 3: Implement the `rig/probe` updates, `rig/tmux/stock.conf`, `rig/tmux/full.conf`, and `rig/orca/run.sh --tmux`**
- [ ] **Step 4: Run `just check`, `just lint-targets`, and live `just orca --tmux stock capability_probe` + `just orca --tmux full capability_probe`**
- [ ] **Step 5: Commit (`feat(rig): add tmux configs, probe osc52-form and Windows mode alignment, and Orca tmux mode`)**

---

### Task 2: Scaffold `rig/xterm` Package, Browser Terminal Bridge, and Pure Node Unit Tests

**Files:**
- Modify: `.gitignore` (ignore `rig/xterm/node_modules/`)
- Create: `rig/xterm/package.json`
- Create: `rig/xterm/package-lock.json`
- Create: `rig/xterm/web/index.html`
- Create: `rig/xterm/src/args.mjs`
- Create: `rig/xterm/src/ready_waiter.mjs`
- Create: `rig/xterm/src/bridge_server.mjs`
- Create: `rig/xterm/src/pty_session.mjs`
- Create: `rig/xterm/test/unit.test.mjs`

**Interfaces:**
- Pins exact `@xterm/xterm` (`6.0.0`), `@xterm/addon-unicode11` (`0.9.0`), `node-pty` (`1.1.0`), and `playwright` (matching local/CI headless Chromium, lockfile committed).
- `web/index.html`: loads browser `@xterm/xterm` CSS + JS and `@xterm/addon-unicode11` from the local HTTP server, opens `Terminal({ allowProposedApi: true, cols: 100, rows: 30, scrollback: 1000, theme: { background: '#122738', foreground: '#e5e9f0' } })`, activates Unicode `'11'`, registers OSC 52 capture (`window.__osc52Payloads`, decoding `52;<selection>;<base64>`), exposes `window.__terminalState()` (returning `{ cols, rows, cellWidthPx, cellHeightPx, mouseTrackingMode, osc52Payloads, activeBufferLine }`), connects WebSocket, and sends `terminal_ready` only after layout/cell metrics are non-zero.
- `bridge_server.mjs` + `pty_session.mjs`: enforces Rule 5 of `docs/testing.md` (starts HTTP/WS server + Playwright Chromium + waits for `terminal_ready` BEFORE attaching `pty.onData` and spawning `node-pty`).
- `ready_waiter.mjs`: condition-based `<log>.ready` + `"kind":"ready"` JSONL waiter and process-exit waiter with bounded failure timeouts and zero sleeps.

- [ ] **Step 1: Install `rig/xterm` npm dependencies and write failing `node --test` unit tests in `rig/xterm/test/unit.test.mjs` (CLI arg parsing, OSC 52 payload decoding, `.ready` waiter, Rule 5 startup ordering)**
- [ ] **Step 2: Run `npm --prefix rig/xterm test` and verify failure (Red)**
- [ ] **Step 3: Implement `web/index.html`, `args.mjs`, `ready_waiter.mjs`, `bridge_server.mjs`, and `pty_session.mjs`**
- [ ] **Step 4: Run `npm --prefix rig/xterm test` and verify all Node unit tests pass (Green)**
- [ ] **Step 5: Commit (`feat(xterm): scaffold xterm.js 6.0.0 browser bridge, pty session, and unit tests`)**

---

### Task 3: Implement `rig/xterm` Scenario Driver, `rig/xterm/run.sh`, and `just xterm` (Layer 3)

**Files:**
- Create: `rig/xterm/src/cell_coords.mjs`
- Create: `rig/xterm/src/scenarios.mjs`
- Create: `rig/xterm/src/harness.mjs`
- Create: `rig/xterm/run.sh` (`chmod +x`)
- Modify: `justfile` (`xterm *args:` -> `./rig/xterm/run.sh "$@"`)
- Modify: `rig/xterm/test/unit.test.mjs` (add scenario assertion and CLI contract tests)

**Interfaces:**
- `just xterm <program> [scenario] [extra_args...]`:
  - Default `[scenario]` for `capability_probe` when omitted: `all` (runs `batch`, `osc52`, `interactive`, and `panic` scenarios sequentially and emits structured JSON summary + probe records to `stdout`), or individual scenarios: `batch`, `osc52`, `interactive`, `panic`, plus `passthrough` for generic future `*_lab` examples.
- `scenarios.mjs` (`interactive` scenario):
  1. Waits for `.ready` sentinel, reads `window.__terminalState()` (`mouseTrackingMode`, `cellWidthPx`, `cellHeightPx`).
  2. Moves pointer across cells with no button pressed (`hover` check: records whether any `moved` mouse event arrived).
  3. Injects real Playwright pointer events at exact cell centers: left click at `(10, 5)`, left drag `(12, 6)` -> `(18, 6)`, wheel down & wheel up at `(10, 5)`, right click at `(15, 7)`, `Shift+click`, `Alt+click`, `Alt+drag` (M0 row 3), `Ctrl+click`, and double click.
  4. Injects real Playwright keyboard events: `ArrowUp`, `F1`, `Control+a`, `Alt+a`, `Alt+Enter`, `Shift+Enter`, `Control+Enter` (and repeats `Alt+Enter`, `Shift+Enter`, `Control+Enter` in a `--push-kitty` sub-check where supported).
  5. Calls `term.paste("alpha\nbeta")` and records `paste` / key-burst behavior (M0 row 16).
  6. Triggers terminal blur and focus (`FocusLost` / `FocusGained`), then sends `Control+c` (`CleanExit`), waits for exit `0`, and verifies `mouseTrackingMode === 'none'` after teardown.
- `scenarios.mjs` (`panic` scenario):
  - Launches interactive probe, waits for `.ready`, presses `F12`, waits for nonzero exit, and verifies `mouseTrackingMode === 'none'` (proving panic-hook teardown restored xterm.js modes).

- [ ] **Step 1: Write failing tests for cell coordinate math, scenario assertions, and `rig/xterm/run.sh` CLI contract (`-h`/`--help`, invalid args, `NO_COLOR`, stdout JSON cleanliness)**
- [ ] **Step 2: Run `npm --prefix rig/xterm test` and verify failure (Red)**
- [ ] **Step 3: Implement `cell_coords.mjs`, `scenarios.mjs`, `harness.mjs`, `rig/xterm/run.sh`, and `justfile` `xterm` recipe**
- [ ] **Step 4: Run `npm --prefix rig/xterm test`, `just xterm capability_probe`, `just check`, and `just lint-targets`**
- [ ] **Step 5: Commit (`feat(xterm): implement Layer 3 Playwright xterm.js scenario driver and just xterm`)**

---

### Task 4: Implement `rig/xterm/run-tmux.sh` + `just xterm-tmux` (Layer 4) and Verify 30 Flake-Free Runs

**Files:**
- Create: `rig/xterm/src/tmux_session.mjs`
- Create: `rig/xterm/run-tmux.sh` (`chmod +x`)
- Modify: `justfile` (`xterm-tmux *args:` -> `./rig/xterm/run-tmux.sh "$@"`)
- Modify: `rig/xterm/test/unit.test.mjs` (add `run-tmux.sh` CLI contract and private tmux server cleanup tests)

**Interfaces:**
- `just xterm-tmux <config> <program> [scenario] [extra_args...]` (`config` must be `stock` or `full`):
  - Launches a private `tmux -L <unique_socket> -f rig/tmux/<config>.conf` server inside `node-pty`, running `<program>` at `100x30`, and kills the private server on teardown (`tmux -L <unique_socket> kill-server`).
  - In `osc52` / `all` scenario inside tmux (M0 Row 12): tests `--osc52-form plain` alone and `--osc52-form tmux` alone (and `both`), reading back BOTH `tmux -L <unique_socket> show-buffer` AND browser `window.__osc52Payloads` so M0 Row 12 (`stock` vs `full`, plain vs passthrough) is measured completely!
  - In `interactive` / `all` scenario inside tmux (M0 Rows 9 & 10): injects click, drag, and wheel events, asserting **zero** mouse events arrive in the probe under `stock` tmux (M0 Row 9) and **exact cell** mouse events arrive in the probe under `full` tmux (M0 Row 10)!

- [ ] **Step 1: Write failing unit & CLI tests for `tmux_session.mjs` and `rig/xterm/run-tmux.sh` (`-h`/`--help`, invalid `<config>`, private socket cleanup)**
- [ ] **Step 2: Run `npm --prefix rig/xterm test` and verify failure (Red)**
- [ ] **Step 3: Implement `tmux_session.mjs`, `rig/xterm/run-tmux.sh`, and `justfile` `xterm-tmux` recipe**
- [ ] **Step 4: Run `just xterm-tmux stock capability_probe` and `just xterm-tmux full capability_probe`, then execute the 30-consecutive-run reliability gate (`30x` `just xterm capability_probe` + `30x` `just xterm-tmux stock capability_probe -- scenario=batch` / `just xterm-tmux full capability_probe`) confirming 0 failures and 0 stray `tmux` servers**
- [ ] **Step 5: Run `just check` and `just lint-targets`, then commit (`feat(rig): add Layer 4 xterm-tmux harness and verify 30 consecutive runs`)**
