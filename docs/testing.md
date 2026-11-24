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

CI retains stable and Rust 1.88 build/test checks on all three platforms.
Feature rows cover every publishable crate: empty, each declared feature alone,
and all features, with `--all-targets`. For the current mouse crate these are
empty/`vt100`/all; for the umbrella they are empty/`mouse`/all. Unix uses the
existing metadata-driven `scripts/ci/check-features.sh`; Windows derives the
same rows from Cargo metadata in Node, called from PowerShell with explicit
failure propagation and no Bash dependency. The lab's `required-features`
keeps its vt100-only example out of empty-feature all-target checks. These
checks prove compilation isolation; the feature-enabled workspace tests run
the shared surface contract against both `LineSurface` and `Vt100Surface`.

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

When `RIG_XTERM_LOG_DIR` is set, the harness also creates a dedicated
`lifecycle-pid<PID>-<sequence>.jsonl` artifact before building or starting a
bridge. Each boundary appends an `enter`, `return`, or `error` record
synchronously, with monotonic `elapsedMs`, `harnessPid`, the session label
(including X11 Shift or Alt), and `ptyPid` once available. The diagnostic file
uses its own collision-safe names and does not consume app archive numbers.
It contains build/scenario/report phases, bridge startup, app readiness,
gestures, termination, PTY exit/write flush, archive, session disposal,
browser/HTTP close, and scratch removal. Direct CLI processes also record
`beforeExit` and `exit` when those events occur. These events are observational;
logging does not change disposal, terminal assertions, or deadlines. Failed
lifecycle appends are ignored so artifact loss cannot skip cleanup, interrupt
PTY exit bookkeeping, or replace an operation's original error.

The last entered boundary is already on disk while its operation is blocked,
even before an app log exists or an archive is copied. Pair it with the
parent test's child `status`, `signal`, and spawn-error fields to distinguish
an aggregate timeout from a normal nonzero exit. On failure, the parent
retains partial stdout in a separate `*-child-outcome.json` artifact. All
phase records stay out of app JSONL, machine stdout, and diagnostic stderr.
With logging unset, no lifecycle file is created. The existing 180-second
mouse aggregate deadline remains unchanged. Windows timeout diagnosis still
requires an instrumented CI run; a final entered phase identifies the next
component to inspect, rather than proving a particular dependency defect.

PTY disposal kills only a child whose exit event has not been observed. On
Windows, an already-exited session instead releases the pinned node-pty 1.1.0
conout worker through its existing drain-and-dispose method, then destroys the
input socket. A running session retains public kill and its worker cleanup;
if kill throws, the harness still attempts worker and input release
independently. Disposal stays synchronous and idempotent: it initiates the
dependency's worker drain without awaiting worker exit or changing the existing
output-flush and exit-wait behavior. A child may exit between the running-state
check and kill, so the running path still tolerates termination errors.

Focused unit cases cover natural exit, live termination, independent cleanup
failures, repeated disposal, and unchanged Unix behavior. A Windows-only
subprocess regression also exercises two naturally exiting real Node PTYs and
one live termination in separate child processes. It subscribes to each actual
worker-exit and input-close event before disposal, requires the owned handles
to close, and rejects `AttachConsole failed` in captured natural-exit stderr.
This runtime check runs on Windows CI; portable fakes alone do not establish
Windows handle release. Fixture phases have labeled bounds within the outer
subprocess deadline. On failure it independently attempts session, worker, input-socket, and captured
child cleanup, preserving the initiating error; forced
teardown never satisfies the successful-closure assertions. The earlier
180-second aggregate timeout remains distinct from the confirmed post-exit
helper defect.

Both direct and tmux harnesses keep Cargo build diagnostics on stderr. When
stderr is redirected, `NO_COLOR` is nonempty, or `TERM=dumb`, the Cargo child
uses plain output even if the parent sets `CARGO_TERM_COLOR=always`. Diagnostic
text and build failures are preserved; stdout remains the JSON report.

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

The mouse laboratory runs **X1 through X14 in both configurations**, with
independent Shift and Alt sessions for X11. Session reports retain the actual
`tmuxServerInfo` version and `mouse` option. Stock pane forwarding is required
even when tmux's own `mouse` option is off. X10 keeps exact pane-relative bytes
and X13 requires keyboard-only `Copy` of `file.rs:12` in every configuration.

X1 checks the app's exact setup and teardown sequences independently of outer
tracking. In tmux 3.8, pane query replies prove the app's modes were reset;
in 3.3a, all missing replies must be explicitly absent with null state.
`terminalStateAfterExit` is sampled after the attached tmux client exits,
when outer tracking is reset. It does not describe tmux's own UI tracking
between app teardown and client detachment. X2 verifies the same cleanup
sequence and outer state after a nonzero panic exit.

X11 requires Alt Down/Drag/Up and exact copied text in 3.3a. In 3.8 it
requires Alt Down/Up, no Drag or Copy, and only a zero-length `Pressed`
selection before release. X14 sends two initial-left/rapid-target click
pairs in one ordered Chromium CDP input batch, then closes one F2 log fence.
All eight press/release commands are queued before awaiting acknowledgments;
this keeps browser or app report round trips out of tmux's click timer.
The four releases divide the log into independently checked gestures:
both initial left-pane clicks must produce Pressed/Clicked, while rapid right
and Control clicks are Up-only in 3.3a and normal Pressed/Clicked in 3.8.
Up-only gestures must produce no click, selection, copy, or forwarding.
The initial clicks use distinct cells outside the multi-click distance, so they
do not accidentally start word selection. The harness assumes Chromium processes input commands in CDP session order;
release order and per-gesture outcomes verify that order in the app log.

```sh
just xterm-tmux stock mouse_lab mouse
just xterm-tmux full mouse_lab mouse
just linux ./rig/xterm/run-tmux.sh stock mouse_lab mouse
just linux ./rig/xterm/run-tmux.sh full mouse_lab mouse
npm --prefix rig/xterm test
```

These four mouse matrices are verified on macOS with tmux 3.8 and the Linux
floor container with tmux 3.3a. Every matrix executes all fourteen scenario
IDs (fifteen fresh sessions), including the measured degradation assertions.

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

`just orca mouse_lab -- --query-modes` builds the laboratory with its `vt100`
feature and drives X1 and X13 through Orca's terminal API. It waits for the
lab's ready sentinel and log record, sends only keyboard input, waits for the
exact `Copy` of `file.rs:12`, quits, and verifies the exit status, setup and
teardown sequences, and all eight actual mode replies in both phases. Setup
requires 1002/1006 set; inactive modes must be reset or permanently reset
(states 2/4), never absent or unsupported. Native JSONL records reach stdout.
The runner closes its terminal on success and failure; failed runs preserve
the log directory named on stderr. Node.js is required for this path.

`--interactive` leaves keyboard input to an external controller and still
returns the native lab log after exit. Automated lab checks use a direct Orca
terminal; private tmux sessions remain available with `--interactive`.
The capability probe remains batch by default. No Orca desktop pointer,
focus control, pointer scenarios, or scroll-preference measurement is used or
claimed by this layer.

```sh
just orca mouse_lab -- --query-modes
just orca mouse_lab --interactive -- --query-modes
node --test rig/orca/test/mouse_lab.test.mjs
cargo test -p rig-probe --test orca_runner_cli
```

The mocked driver suite checks readiness and log conditions, keyboard-only
Copy, exact lifecycle evidence, rejection of missing/absent/enabled inactive
mode replies, and terminal cleanup on send, exit, or missing-Copy failure.
The CLI suite checks feature-aware lab builds, argument forwarding, native
records for external control, and the unchanged probe batch default.

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

The mouse laboratory uses the same VM lifecycle, independently of the existing
probe and M0 matrix. Its default `mouse` scenario executes X1 through X14 in
fresh iTerm2 sessions; individual `x1` through `x14` names select one
scenario. The build selects `ratatui-mouse` and its `vt100` feature. All
assertions read the laboratory's native JSONL, using a logged F2 key followed
by its report as a FIFO fence. Modifier gestures wait for CoreGraphics to
report the held keys, and native selection keeps those keys held until the
clipboard confirms its update. Gesture positions combine the logged content
rectangles with CoreGraphics window bounds and the guest terminal grid. A
single acknowledged VNC connection retains pointer position, held buttons, and
modifiers across log waits. It owns input before issuing down calls, so a
failure after delivery still triggers release. Every held key/button release,
disconnect, and API shutdown is attempted independently. Its owner closes and
reaps the child even if closing stdin fails, escalating a shutdown timeout to
a bounded kill/reap. Cleanup failures and nonzero child exits fail the driver,
retaining any original scenario or startup error as the cause. Real subprocess
and pipe regressions cover delivered-down errors, each cleanup failure, and
broken stdin during ordinary and startup cleanup. The lab's physical Shift+1
(`!`) panic and `q` exit are
independent of the probe's F12 binding.

```sh
just iterm2 mouse_lab mouse -- --query-modes
just iterm2 mouse_lab mouse --tmux stock -- --query-modes
just iterm2 mouse_lab mouse --tmux full -- --query-modes
python3 -m unittest discover -s rig/iterm2/tests -p 'test_*.py'
```

X1 requires the exact setup and teardown sequences and actual mode replies. X2
verifies a nonzero panic exit, logged cleanup bytes, and an independent
post-exit mode snapshot: the panic hook does not issue lab teardown queries.
Inside tmux, that snapshot describes the wrapper's pane PTY, rather than the
outer iTerm2 state after detachment. X3 fences hover before and after pane
pass-through. X4 through X10 check ownership, exact character/word/soft-wrap
copying, history row 030 beyond the viewport, signed wheel steps, wheel during
drag, and exact pane-relative bytes. X12 checks alternate-screen arrow
forwarding; X13 is keyboard-only and must copy `file.rs:12`. X14 checks
iTerm2's default right-click and Control-click context menus with no app mouse
events, dismissing each menu before its fence rather than waiting for an event
the terminal intentionally suppresses.

X11 runs Shift, Command, Option, and Option+Command in separate sessions.
Shift and Command routing is checked in the app's decoded events (Command is
represented by Alt). Direct Command drag copies exact app text; tmux 3.8
requires Alt Down/Up, no Drag or Copy, and at least one right-target
zero-length Pressed selection. Every non-null selection must satisfy that
condition; missing or exclusively null observations fail. X13 still supplies
the complete keyboard equivalent in both tmux configurations. Present
mode-reply state `0` for unsupported legacy formats is
retained as unsupported, never treated as absence or reset. Native Option
contiguous selection and Option+Command rectangular selection are checked
against exact guest clipboard text and must produce no app selection or
forwarding. The aggregate repeats identical raw wheel-button pulses with
natural scrolling on, after the ordinary off run: left/right steps change from
`+3/-3` to `-3/+3`, matching the received ScrollDown/ScrollUp inversion.
Reports retain the preference phase and raw VNC buttons so this reversal is
explicit. Clipboard text, scrolling preferences, the test-only copy-selection
setting, and iTerm2 defaults are restored on success and failure. The saved
iTerm2 domain is imported through the preferences API so cached test values
cannot overwrite the restored settings. A failed import retains the guest
backup, attempts the independent global-setting cleanup, and reports failure.
A later run refuses to overwrite that recovery backup and prints the guest
import command needed to recover it. Signal handling unwinds guest
restoration before stopping the VM. A test-only `NoSyncTipsDisabled`
preference prevents the timed Tips opt-in dialog from blocking launch and is
restored with the complete iTerm2 preferences backup.

Verified on iTerm2 3.7.7 in the macOS 15 guest with tmux 3.8: all fourteen
scenario IDs pass directly and in both tmux configurations, with eighteen
fresh app sessions per configuration (54 total). Independent success and
forced-failure audits for Option and Option+Command clipboard timeouts,
Control-barrier failure, and incorrect copied text all restore the complete
canonical preferences, global settings, clipboard text, and actual guest
modifier flags to their saved state. No preference keys are excluded from the
comparison. An injected preferences-import failure also preserves the saved
backup through a refused subsequent backup attempt, restores independent
globals, and recovers the complete saved state when the import succeeds.

Set `RIG_ITERM2_LOG_DIR` to retain every native session JSONL, the aggregate
JSON report, and a failure screenshot. Screenshots diagnose launch or menu
problems; no assertion compares pixels. Lab scenarios support direct iTerm2,
stock tmux, and full tmux; the existing probe retains its `tmux -CC` paths.

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

The workflow runs `node rig/xterm/src/harness.mjs mouse_lab mouse` from
PowerShell, which builds the example with `vt100` and executes all X1-X14
through ConPTY. The same lab suite runs directly on macOS and Linux, plus
both Unix tmux configurations. Existing capability-probe comparisons remain
separate and unchanged. Windows artifacts upload on every outcome and retain
native mouse events, scenario evidence, any-motion/SGR setup, ignored hover,
and clean/panic restoration observations.

**Current M1 gate (2026-11-22): pending Windows execution.** The new workflow
has not run on a pushed branch. Before the first push, obtain explicit push
authorization; then run `just windows --wait`, inspect all three jobs of that
run and its downloaded Windows logs, and fix failures. Cross-target Clippy and
M0's historical ConPTY measurements do not replace this execution.

`just windows` (`./rig/windows/run.sh`) inspects the current branch's latest
`ci.yml` run via `gh`, optionally waits for completion (`--wait`), and
downloads the Windows job logs and `RIG_XTERM_LOG_DIR` event-log artifacts
into `target/windows-ci/` so any failure can be inspected locally without
rerunning CI.

A local Windows virtual machine was considered and set aside: Windows needs a
paid license to be used properly, and CI runners come already licensed.

## Human release judgments

Agents run every correctness check above. A person supplies only the following
subjective judgments with the mouse laboratory; these are separate from the
pending Windows runtime gate. No agent substitutes a passing assertion for
scroll feel or visual legibility.

| Judgment | Current default | Reviewer | Date | Result / adjustment |
| --- | --- | --- | --- | --- |
| Edge scrolling interval feels controllable | 50 ms | Pending | Pending | Pending |
| Fastest edge scrolling remains controllable | Maximum 10 rows per tick | Pending | Pending | Pending |
| Wheel distance feels appropriate | 3 rows per notch | Pending | Pending | Pending |
| Selection highlight is legible on a dark background | Reversed video | Pending | Pending | Pending |
| Selection highlight is legible on a light background | Reversed video | Pending | Pending | Pending |
| Selection highlight is legible with `NO_COLOR` | Reversed video, no fixed colors | Pending | Pending | Pending |

Use `cargo run -p ratatui-mouse --example mouse_lab --features vt100` for an
interactive session, or set `NO_COLOR=1` for the color-disabled judgment. The
host terminal supplies the background. Its focus, select-mode, and scrolling
keys are documented in the [crate README](../crates/ratatui-mouse/README.md).
Record the person's name, actual review date, accept/change decision, and any
new default in the table when performed. Reverify each changed default's unit
tests and affected terminal scenarios before release. Until every row is
reviewed and required runtime layers pass, the branch is not release-ready.

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
| M1 mouse library/laboratory | Local macOS/Linux, xterm.js, stock/full tmux, live Orca X1/X13, and iTerm2 VM matrices verified; new Windows lab runtime remains pending |
| M1 CI feature and mouse matrix additions | Configured for all three platforms; remote execution pending authorization and push |
| M1 human release judgments | Pending named reviewer/date for all six rows above |
