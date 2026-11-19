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
thirty runs in a row gave identical results.

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

### 1. Unit and escape-sequence tests

Decisions are pure functions (see `rules/tui.md`), so most of the toolkit is
tested by calling functions. What a part writes to the terminal is a pure
function too, so the rules in `terminals.md` are assertions on bytes: the mouse
setup contains modes 1000, 1002 and 1006 and never 1003, no mouse mode is
reset before teardown, a paste is bracketed.

Linux runs in a container at the minimum Rust version, which checks that
floor at the same time.

### 2. Cross-target lint

The Windows and Linux targets are installed with rustup, and clippy runs
against each from the development machine. No Windows toolchain is needed,
because nothing is linked. It catches a `cfg(windows)` branch that no longer
compiles, which is the most common way a cross-platform crate breaks.

### 3. The xterm.js harness

xterm.js is the terminal inside Orca, so this layer carries the most weight.
A small Node program opens a real xterm.js in a headless browser, at the
version Orca ships and with the same Unicode add-on, and connects it to one of
the toolkit's example programs running in a pseudo-terminal. The browser
delivers real mouse and keyboard events to xterm.js, xterm.js encodes them as
it would for a person, and the test reads what the program received.

It uses the browser build of xterm.js, not the headless one: the code that
turns a click or a key press into bytes exists only in the browser build, and
that code is what has to be tested. On Windows the pseudo-terminal is ConPTY,
so this layer is also the Windows terminal test.

Proved: a click, a drag, the wheel, clicks with Shift, Option and Control, a
right click, a double click, arrow and function keys, Control and Alt chords,
a bracketed paste and focus changes all arrived as the expected bytes, with
thirty identical runs in a row at about four seconds each.

### 4. tmux in the xterm.js harness

The same harness starts the example program inside a private tmux server
(its own socket, its own configuration file), once with tmux's stock
configuration and once with the full one from `terminals.md`.

### 5. Orca

Orca's command line can start a terminal, type into it and read it back. The
capability probe runs in a live Orca terminal that way and its answers are
compared with `terminals.md`. This is the check that the harness in layer 3
still matches the real thing: if Orca changes its xterm.js version or
settings, this layer says so.

Proved: the probe ran in an Orca terminal, alone and inside tmux, and
reported xterm.js 6.0.0.

### 6. iTerm2 in a virtual machine

iTerm2 is a desktop app. Driving it on the development machine itself does
not work without a person: macOS asks for approval the first time an app is
opened, and asks again before one program may watch or control another, and
only someone at the screen can answer. So iTerm2 runs in a macOS virtual
machine that an agent controls completely.

The virtual machine exposes its screen and input over VNC from outside the
guest. A test starts the example program in iTerm2, moves the pointer, clicks,
drags, scrolls and types through VNC, which the guest sees as real hardware
events, and reads the program's log from a folder shared with the host. A
screenshot is one command away when something needs looking at.

The virtual machine is prepared once so that no prompt can appear in a run:
iTerm2 installed with its download flag cleared, its confirmation for running
a script answered, the local network prompt answered. It is then kept and
reused.

Proved: iTerm2 3.7.7 was installed in a macOS 15 virtual machine, its prompts
answered through VNC, the probe run alone and inside tmux, and a click, a
drag, the wheel and modified clicks injected through VNC arrived in the
program at the expected cells.

It uses [Tart](https://github.com/openai/tart) to run the virtual machine.
Tart is source-available under the Functional Source License, not open
source. It is a development tool only: no crate depends on it.

### 7. Windows on CI runners

Windows is the one platform with no local run. Layers 1, 3 and the build at
the minimum Rust version run on GitHub's hosted Windows runners on every pull
request, and that is where Windows behavior is proved. Locally, layer 2
catches Windows code that no longer compiles before a push.

An agent still runs this layer itself: it pushes the branch, waits for the
Windows jobs, and reads their logs. Because the loop is slower than a local
run, Windows-only code is kept small and behind one module per concern (see
`rules/rust.md`), and the Windows jobs upload the example programs' event
logs so a failure can be read without rerunning it.

A local Windows virtual machine was considered and set aside: Windows needs a
paid license to be used properly, and CI runners come already licensed.

## Status

| Piece | State |
| --- | --- |
| Capability probe (first version, throwaway) | Run in Orca, iTerm2 and tmux. Its results are in `terminals.md`. |
| xterm.js harness | Proved as a throwaway. To be built in the repository. |
| iTerm2 virtual machine | Proved by hand. The scripted version is to be built. |
| Linux container run | Proved with the workspace as it is |
| Cross-target lint | Proved against the planned dependencies |
| CI | Not set up |
