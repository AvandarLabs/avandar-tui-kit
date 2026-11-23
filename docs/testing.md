# Testing

How the toolkit is tested, where each kind of test runs, and who runs it.
[`terminals.md`](terminals.md) says what has to work; this page says how that
is proved.

## The rules

1. **An agent can run every layer.** No test waits for a person to start it,
   watch a screen, or click a prompt. Where a real desktop app is involved, it
   runs somewhere an agent has eyes and hands: a virtual machine it controls,
   or an app with a command-line interface. Whoever makes a change runs the
   layers that change touches before calling it done. CI repeats them as the
   gate on a pull request; it is never the first place a failure is seen.
2. **Reliable, or not automated.** A test that fails for any reason other than
   the code under test is fixed or deleted. It is never retried until it
   passes, and never marked flaky and left in.
3. **Wait on a condition, never on the clock.** A test waits for something
   the program or the terminal reports (a line in a log, a terminal mode being
   on), with a timeout that only ever means failure. A fixed sleep is a bug.
4. **Programs under test say what they received.** The example programs write
   a log of every event they got and every decision they made. Tests assert on
   that log and on the terminal's own state (its modes, its text). They never
   compare pixels. Screenshots are for debugging.
5. **Start the program after the terminal is listening.** Output a program
   writes before anything is reading it is lost, and with it the mode changes
   the test depends on.

Rules 3 and 5 were learned the direct way. The first version of the xterm.js
harness below used one short sleep and attached its output reader a moment
after starting the program, and two runs in ten failed. With both fixed,
thirty runs in a row gave identical results, with zero fixed sleeps anywhere
in `rig/`.

## The layers

| # | Layer | What it proves | Where it runs | In CI |
| --- | --- | --- | --- | --- |
| 1 | Unit and escape-sequence tests | Every decision, and the exact bytes each part writes and reads | `cargo test` on macOS, in a Linux container, on Windows | Yes, on all three |
| 2 | Cross-target lint | Windows and Linux code type-checks and lints before it leaves a Mac | `cargo clippy --target` for the Windows and Linux targets | No: CI builds natively |
| 3 | xterm.js harness | A real xterm.js, at the version Orca ships, gets real mouse and key events and the program receives the right bytes | Node and a headless browser, on macOS, Linux and Windows | Yes, on all three |
| 4 | tmux in the xterm.js harness | The same, through tmux with its stock and its full configuration | macOS and Linux | Yes, on both |
| 5 | Orca | The capability probe gives the expected answers in a live Orca terminal | Anywhere Orca runs, through Orca's command line | No: it needs Orca |
| 6 | iTerm2 in a virtual machine | A real iTerm2 gets real pointer and keyboard events, alone and through tmux | A macOS virtual machine on an Apple Silicon Mac | No: it needs nested virtualization |
| 7 | Windows on CI runners | The Windows build behaves, through the real pseudo-console | GitHub's hosted Windows runners | Yes: CI is the only place it runs |

A change runs the layers it can affect. Pure logic stops at layers 1 and 2.
Anything that writes to or reads from the terminal runs all of them.

What is left for a person is a short checklist before a release, for
judgments no assertion can make: whether scrolling feels right, whether the
colors read well. Nothing on it is a correctness check.

## How to run each layer

Every `justfile` recipe is a single command you can also run directly:

| Layer | `just` recipe | Single command |
| --- | --- | --- |
| 1 (host) | `just test` | `cargo test --workspace --all-features` |
| 1 (Linux floor) | `just linux <command>` | `./rig/linux/run.sh <command>` (for example `just linux just check`) |
| 2 | `just lint-targets` | `cargo clippy --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --target x86_64-unknown-linux-gnu -- -D warnings` |
| 3 | `just xterm <program> [scenario]` | `./rig/xterm/run.sh <program> [scenario]` |
| 4 | `just xterm-tmux <config> <program> [scenario]` | `./rig/xterm/run-tmux.sh <config> <program> [scenario]` (`config` is `stock` or `full`) |
| 5 | `just orca [--tmux <stock\|full>] <program>` | `./rig/orca/run.sh [--tmux <stock\|full>] <program>` |
| 6 | `just iterm2 <program> [scenario]` | `./rig/iterm2/run.sh <program> [scenario]` (also accepts `--tmux <stock\|full\|cc-stock\|cc-full>`) |
| 7 | `just windows` | `./rig/windows/run.sh` (also accepts `--wait`, `--branch <name>`, `--run-id <id>`) |

### 1. Unit and escape-sequence tests

Decisions are pure functions (see `rules/tui.md`), so most of the toolkit is
tested by calling functions. What a part writes to the terminal is a pure
function too, so the rules in `terminals.md` are assertions on bytes: the mouse
setup contains modes 1000, 1002 and 1006 and never 1003, no mouse mode is
reset before teardown, a paste is bracketed.

Linux runs in `rig/linux` (`rust:1.88-bookworm` with Node 22, Playwright
Chromium, and `tmux 3.3a`), which checks the Rust 1.88 and `tmux 3.3` floors
locally (`just linux`) and in CI using the same image.

### 2. Cross-target lint

The Windows (`x86_64-pc-windows-msvc`) and Linux (`x86_64-unknown-linux-gnu`)
targets are installed with rustup, and `just lint-targets` runs clippy against
both from the development machine. No Windows toolchain is needed, because
nothing is linked. It catches a `cfg(windows)` branch that no longer compiles,
which is the most common way a cross-platform crate breaks.

### 3. The xterm.js harness

xterm.js is the terminal inside Orca, so this layer carries the most weight.
`rig/xterm` opens the browser build of `@xterm/xterm` `6.0.0` with
`@xterm/addon-unicode11` `0.9.0` in headless Chromium via Playwright and
connects it over WebSocket to an example program running in a `node-pty`
pseudo-terminal. Playwright delivers real mouse, wheel, keyboard, paste, and
focus events to xterm.js, xterm.js encodes them as it would for a person, and
`./rig/xterm/run.sh` emits a JSON probe report on `stdout` while
`node rig/xterm/src/expected_compare.mjs <report-or--> rig/expected/<config>.json`
normalizes and verifies probe output against
`rig/expected/xterm-{macos,linux,windows}.json`.

`just xterm mouse_lab` builds only `ratatui-mouse`'s example with `vt100`
and runs X1 through X14, or one scenario by its `x1` through `x14` name.
The default aggregate `mouse` runs fifteen fresh managed sessions: X11 uses
independent Shift and Alt sessions. X1 checks drag tracking plus SGR on Unix,
any-motion plus SGR on Windows, and restored tracking after exit. Windows
query silence is recorded as absence, never interpreted as reset. X3 checks
hover before and after pane pass-through with a FIFO F2 event/report fence.
The remaining scenarios assert target ownership, exact copied characters and
soft-wrap joining, timed history selection, signed wheel steps, wheel during
a drag, exact pane-relative mouse bytes and arrow keys, modifier routing,
keyboard-only selection, and button/modifier preservation.

Gesture coordinates come from the lab's logged layout. Press/release and
wheel actions wait for their corresponding logged event and report; repeated
clicks wait for each previous press to be processed. X7 holds until the logged
selection reaches history row 030, beyond the initial viewport, before
release. Its copy is checked against every original history row through the
final endpoint. X11 checks exact native Shift selection on Linux and Windows
before the keyboard fence, because xterm clears native selection on user input;
that fence then proves no app selection or forwarding occurred. macOS Shift
and all platforms' Alt gestures must copy exact app text without forwarding.
X13 injects only keys and requires `file.rs:12` with no logged mouse input.

Run the direct suite on both available platforms:

```sh
just xterm mouse_lab mouse
just linux ./rig/xterm/run.sh mouse_lab mouse
npm --prefix rig/xterm test
```

The native lab records remain in the JSON report and in `RIG_XTERM_LOG_DIR`
archives without probe-schema normalization, including when an assertion
fails. Each session reports its scenario ID, assertion result, platform,
readiness and exit terminal states, and gesture evidence. The browser helper
reads encoding from the pinned xterm.js mouse service because its public mode
state exposes tracking but not SGR encoding.

It uses the browser build of xterm.js, not the headless one: the code that
turns a click or a key press into bytes exists only in the browser build, and
that code is what has to be tested. On Windows the pseudo-terminal is ConPTY,
so this layer is also the Windows terminal test.

Proved: **30 consecutive identical runs** of `just xterm capability_probe`
natively on macOS and **30 consecutive identical runs** inside the `rig/linux`
floor container (`rustc 1.88.0`), with zero fixed sleeps anywhere in the rig,
plus CI verification on macOS, Linux, and Windows (`windows-2025`).

### 4. tmux in the xterm.js harness

`just xterm-tmux` (`./rig/xterm/run-tmux.sh`) starts the example program inside
a private `tmux` server (its own socket and no user config), once with
`rig/tmux/stock.conf` (an empty file) and once with `rig/tmux/full.conf`
(`mouse on`, `set-clipboard on`, `allow-passthrough on`, `extended-keys on`,
`focus-events on`), and emits a JSON probe report on `stdout`;
`expected_compare.mjs` normalizes and verifies that output against
`rig/expected/xterm-tmux-{stock,full}-{macos,linux}.json`.

Proved: **30 consecutive identical runs** in `stock` and **30 consecutive
identical runs** in `full` both natively on macOS (`tmux 3.8`) and inside the
`rig/linux` container (`tmux 3.3a`) - `90/90` consecutive runs combined with
Layer 3 on each platform.

### 5. Orca

`just orca` (`./rig/orca/run.sh`) creates an unfocused live Orca terminal
through Orca's command line (`orca terminal create` without stealing keyboard
focus), runs the example program alone or inside a private `tmux` server
(`--tmux <stock|full>`), and emits JSONL probe records on `stdout`.
`expected_compare.mjs` normalizes and verifies that output against
`rig/expected/orca.json` and `rig/expected/orca-tmux.json`, while
`rig/probe/tests/expected_baselines.rs` asserts that `rig/expected/orca.json`'s
`xtversion` matches `@xterm/xterm` in `rig/xterm/package.json`.

Proved: `capability_probe` ran in live Orca `1.4.258`, alone and inside `tmux
3.8`, reported `xterm.js(6.0.0)`, and matched both baselines.

### 6. iTerm2 in a virtual machine

iTerm2 is a desktop app. Driving it on the development machine itself does
not work without a person: macOS asks for approval the first time an app is
opened, and asks again before one program may watch or control another, and
only someone at the screen can answer. So iTerm2 runs in a macOS virtual
machine (`tuikit-macos`) that an agent controls completely.

One-time preparation of the `tuikit-macos` Tart guest:

- Apple Silicon macOS 15 guest image in Tart named `tuikit-macos`.
- Automatic login enabled, with screen saver and screen lock disabled so a
  cold boot lands directly on an unlocked desktop.
- macOS Screen Sharing / VNC enabled so loopback VNC connections succeed
  without interactive approval.
- iTerm2 `3.7.7` installed at `/Applications/iTerm.app` with
  `com.apple.quarantine` cleared, first-run script and automation prompts
  approved, and the local-network prompt answered.
- Homebrew `tmux` (`3.8`) installed in the guest for `stock`, `full`, and
  `tmux -CC` runs.
- macOS Sequoia click-wallpaper-to-show-desktop and notification banners
  quieted so pointer events never hide the terminal window.

`just iterm2` (`./rig/iterm2/run.sh`) boots `tuikit-macos` headless with
`tart run --no-graphics --no-audio --no-clipboard --vnc-experimental` and a
VirtioFS shared directory, waits on `tart exec` and RFB handshake readiness
(zero fixed sleeps), calibrates the VNC Option modifier before key chords,
backs up and restores guest `defaults` (`com.googlecode.iterm2` and
`NSGlobalDomain`) around every run, captures a VNC screenshot automatically on
failure, emits a JSON probe report on `stdout`, and stops the VM cleanly on
exit. `expected_compare.mjs` verifies that output against the four iTerm2
baselines in `rig/expected/`: `iterm2.json` (no `--tmux` flag),
`iterm2-tmux-stock.json` (`--tmux stock`), `iterm2-tmux-full.json`
(`--tmux full`), and `iterm2-tmux-cc.json` (`--tmux cc-full`; `--tmux cc-stock`
is also exercised during the `matrix` sweep).

Proved: **10 consecutive cold-start runs** from a stopped `tuikit-macos` VM
(`10/10` identical capability digests), plus full matrix verification across
iTerm2 alone, `stock` and `full` `tmux`, and `tmux -CC` against those four
`rig/expected/iterm2*.json` baselines.

It uses [Tart](https://github.com/cirruslabs/tart) to run the virtual machine.
Tart is source-available under the Functional Source License, not open
source. It is a development tool only: no crate depends on it.

### 7. Windows on CI runners

Windows is the one platform with no local run. Layers 1, 3, the Rust 1.88
floor build, and the `.cmd` shim round-trip test run on GitHub's hosted
`windows-2025` runners on every pushed branch and pull request, checking live
output against `rig/expected/xterm-windows.json`. Locally, Layer 2
(`just lint-targets`) catches Windows code that no longer compiles before a
push.

`just windows` (`./rig/windows/run.sh`) inspects the current branch's latest
`ci.yml` run via `gh`, optionally waits for completion (`--wait`), and
downloads the Windows job logs and `RIG_XTERM_LOG_DIR` event-log artifacts
into `target/windows-ci/` so any failure can be inspected locally without
rerunning CI.

A local Windows virtual machine was considered and set aside: Windows needs a
paid license to be used properly, and CI runners come already licensed.

## Status

| Piece | State |
| --- | --- |
| `rig/probe` (`capability_probe`, VT reply parsers, `.cmd` shim tests) | Built and verified across all 13 configurations |
| `rig/xterm` (`xterm.js 6.0.0` + Playwright + `node-pty` harness) | Built and verified (30/30 consecutive runs on macOS and Linux, plus Windows ConPTY in CI) |
| `rig/tmux` (`stock.conf` and `full.conf` private-server runner) | Built and verified (30/30 consecutive runs per config on macOS `tmux 3.8` and Linux `tmux 3.3a`) |
| `rig/orca` (unfocused live Orca terminal runner) | Built and verified against Orca `1.4.258` (`orca.json`, `orca-tmux.json`) |
| `rig/iterm2` (headless `tuikit-macos` Tart + VNC driver) | Built and verified (10/10 cold-start runs across iTerm2, `tmux`, and `tmux -CC`) |
| `rig/linux` (`rust:1.88-bookworm`, Node 22, Chromium, `tmux 3.3a` container) | Built and verified locally (`just linux`) and in CI |
| `rig/expected` (all 13 normalized capability baselines) | Built and verified in workspace tests and live CI checks |
| Cross-target lint (`just lint-targets`) | Built and verified for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` |
| `rig/windows` (`just windows`) and GitHub Actions CI (`.github/workflows/ci.yml`) | Built and verified across Linux, macOS, and Windows (`windows-2025`) |
