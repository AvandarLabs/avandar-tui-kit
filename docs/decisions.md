# Decisions

One section per decision worth remembering: what was chosen, what it rules
out, and what would make it worth revisiting. Add to the bottom; do not rewrite
history.

## The repository is a workspace of crates behind one umbrella

**Decision.** avandar-tui-kit is a cargo workspace. Each building block is its
own crate under `crates/`, and the `avandar-tui-kit` crate re-exports them
behind features, with none on by default.

**Why.** The apps the toolkit serves need very different subsets. A
menu-driven app with no embedded terminal must not compile a terminal emulator
or the agent layer, and a crate boundary enforces that better than feature
gates inside one crate: each part's dependencies, tests and docs stay its own.
The umbrella keeps the common case to one dependency line.

**Consequences.** Shared settings (version, edition, minimum Rust version,
lints, dependency versions) live once in the root `Cargo.toml`. Releases
publish several crates in dependency order. The name `avandar-tui` stays
unused: it reads as a terminal app for the Avandar product.

**Revisit when.** Two crates are always used together and their separation
costs more than it saves.

## The toolkit started from quickstarter, and the app scaffold was removed

**Decision.** The repository was generated with quickstarter's `rust:cli`
template plus its `rust-tui` capability, for the agent rules, the
`docs/rules/` files and the agent skills setup. The single command-line app it
generated (clap commands, a TOML config, prompts, a theme and a small TUI) was
then removed.

**Why.** The toolkit is a set of library crates, not an app. The scaffold's
TUI modules are still a source for the inventory, and they remain readable in
quickstarter's `templates/capabilities/rust-tui/` and in this repository's
first commit.

**Consequences.** The command-line rules (`docs/rules/cli-ux.md` and
`docs/rules/cli-rust.md`) went with the app. `docs/rules/tui.md` now states
what every crate promises an app instead of how one app is built.

## Library APIs return typed errors

**Decision.** Public functions return an error type the caller can match on,
not `anyhow::Error`. Every fallible public function documents `# Errors`.

**Why.** An app that gets an `anyhow::Error` back can only tell failures apart
by their message. The scaffold allowed `missing_errors_doc` and
`missing_panics_doc` because it was a binary; a published library owes its
callers those sections, so the allowances were dropped.

**Revisit when.** Never for public APIs. Tests and examples may use `anyhow`.

## The minimum Rust version is the one ratatui sets

**Decision.** `rust-version` is 1.88, the floor ratatui 0.30 requires.

**Why.** The scaffold declared 1.85, which ratatui 0.30.2 already rejects. No
crate here needs anything newer yet, and a lower floor serves more apps.

**Revisit when.** A crate needs a newer language or library feature, or the
apps that consume the toolkit settle on a newer toolchain. Raising it is a
breaking change for anyone on an older compiler, so it waits for a release
that is breaking anyway.

## `Cargo.lock` is committed

**Decision.** The lockfile is tracked in git.

**Why.** It makes every contributor and every CI run build the same
dependency graph, which is cargo's current recommendation for libraries too.
Apps that depend on these crates ignore it and resolve their own.

## `just` is the task runner, and it is optional

**Decision.** Tasks live in a `justfile`, and every recipe is a single command
a person could also type by hand.

**Why.** Cargo has no place to hang project tasks (lint with the flags we
mean, build the docs strictly, update the agent skills), so a Rust project
either invents a script directory, a `cargo xtask` crate, or uses a task
runner. `just` is itself a Rust tool (`cargo install just`), a recipe costs one
line, and `just --list` makes the tasks discoverable. A `cargo xtask` crate
earns its keep when a task needs real Rust code (codegen, packaging, a release
pipeline); a whole crate that shells out to `cargo clippy` does not.

**Consequences.** Nobody is blocked by not having `just`: `just lint` is
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.
Recipes take their arguments through `set positional-arguments` and `"$@"`
rather than just's own interpolation, so the file stays readable as shell.

**Revisit when.** A task needs logic rather than a command line. That is the
moment `cargo xtask` starts paying for itself.

## crossterm is the only backend

**Decision.** The toolkit supports crossterm and no other ratatui backend.
Public APIs take crossterm's event types directly, with no abstraction over
backends.

**Why.** A backend is the library an app uses to switch on raw mode, read
input and write output. It does not decide which terminals an app works in:
every backend writes the same escape sequences to every terminal. crossterm
is ratatui's default, runs on macOS, Linux and Windows, and is what every
source app uses. termion (Unix only), termwiz and termina would add an
abstraction layer and a test matrix without making any terminal work better.

**Consequences.** No backend feature flags and no wrapper event types. An app
built on another backend cannot use the input-handling crates.

**Revisit when.** Never on speculation. Only if an app the toolkit must serve
is committed to another backend.

## iTerm2 and xterm.js are the first-class target terminals

**Decision.** Two terminals are first-class: iTerm2, and xterm.js (the
emulator inside Orca and VS Code). Both are tested thoroughly in this
repository, and neither is a fallback for the other. Other terminals are best
effort and untested. See `docs/terminals.md`.

**Why.** These are the terminals the toolkit's apps run in. Orca runs on
xterm.js, and working there is the top priority. iTerm2 has quirks that have
already broken the source apps (a mouse-mode reset switching reporting off
entirely), so it needs tests of its own rather than an assumption that "xterm
compatible" covers it.

**Consequences.** Every crate that touches terminal I/O is tested in both, at
the layers `docs/terminals.md` describes: exact escape sequences in
`cargo test`, a headless xterm.js harness in CI, scripted iTerm2 sessions, and
a manual checklist per release. Defaults never rely on a capability one of the
two lacks: key bindings work without the kitty keyboard protocol, and copying
works without OSC 52. The repository gains a small Node harness for xterm.js
and a Python script for iTerm2, as test tooling only; no published crate
depends on either.

**Revisit when.** The apps this toolkit serves move to another terminal, or
one of these two stops being used.

## Eight building-block crates, split along heavy dependencies

**Decision.** The umbrella crate re-exports eight building blocks:
`ratatui-mouse`, `avandar-tui-term`, `avandar-tui-pty`, `avandar-tui-agent`,
`avandar-tui-widgets`, `avandar-tui-view`, `avandar-tui-charts` and
`avandar-tui-tasks`. The table in `docs/architecture.md` says what each owns.

**Why.** The inventory found about 175 extractable elements, roughly twice
the scope of the first plan. The split follows the dependencies that are
costly to compile or ship (a terminal emulator, a pseudo-terminal library, a
markdown parser, a fuzzy matcher, a tree widget, an async runtime), so an app
pays only for what it turns on. Widgets that need nothing beyond ratatui stay
together in one crate, with the heavier ones behind features.

**Consequences.** A crate is created when its spec is written, not ahead of
it. `ratatui-mouse` keeps a generic name because it is useful to any ratatui
app.

## Mouse pass-through follows terminal conventions

**Decision.** In an embedded terminal pane, mouse events go to the program
inside in the format it asked for. Holding Shift selects text instead of
passing the event on. When a full-screen program did not ask for mouse input,
the wheel sends it arrow keys.

**Why.** That is what iTerm2 and xterm.js themselves do, so a pane behaves
like the terminal people already know.

## tmux is first-class

**Decision.** tmux is a first-class target inside both first-class terminals:
tmux in iTerm2 and tmux in xterm.js are each tested. The minimum supported
tmux is 3.3.

**Why.** The toolkit's apps are often run over SSH inside tmux. tmux is a
terminal emulator of its own between the app and the real terminal, and its
defaults turn off most of what a mouse-driven app relies on: with the stock
configuration the app receives no mouse events at all, and the clipboard and
extended keys depend on options the user may not have set. An app that is
only tested outside tmux breaks there in ways nobody sees until they are on a
remote machine.

**Consequences.** tmux is tested with its stock configuration and with a full
one (mouse, clipboard, passthrough, extended keys and focus events on). The
toolkit must be fully usable from the keyboard in the stock configuration.
The clipboard sends both the plain OSC 52 form and the passthrough-wrapped
form. tmux does not run natively on Windows, so this applies on macOS and
Linux; under WSL the app is a Linux build.

**Revisit when.** A tmux older than 3.3 has to be supported, which would mean
dropping the passthrough route.

## macOS, Linux and Windows are all first-class

**Decision.** The toolkit is cross-platform in fact, not in principle: every
crate builds, passes its tests and behaves the same on macOS, Linux and
Windows, and CI runs on all three.

**Why.** The source apps were written on macOS and assume it throughout:
`pbcopy`, `open`, AppleScript, `/dev/tty`, Unix process groups, a login shell.
A toolkit that carries those assumptions forward is a macOS toolkit. Fixing
them once here is the point of extracting the parts.

**Consequences.** No crate assumes Unix. Anything Unix-only gets a Windows
path with the same behavior behind the same API: console handles for
`/dev/tty`, a job object for a process group, ConPTY for a pseudo-terminal,
`clip.exe` and `start` for the clipboard and opener. Because `unsafe` is
forbidden, platform calls go through a maintained crate, never through
hand-written bindings. Windows puts its pseudo-console between a program and
the terminal, so terminal behavior is verified on Windows separately. iTerm2
exists only on macOS, so on Linux and Windows the first-class terminal is
xterm.js.

**Revisit when.** A part cannot be given equivalent behavior on one platform.
Then the gap is documented in that crate, not silently left out.

## iTerm2 tests run locally, not in CI

**Decision.** The scripted iTerm2 tests run on a Mac with iTerm2 installed,
through their own `just` recipe, and must pass before a release. They are not
part of CI.

**Why.** iTerm2 is a desktop app that needs a logged-in graphical session.
Driving it on a hosted CI runner is fragile, and a flaky required check
teaches everyone to ignore red. Everything that can be checked without the
app (the exact escape sequences iTerm2's rules require) is in `cargo test`
and does run in CI.

**Revisit when.** A reliable way to run iTerm2 on a CI runner exists.

## Prompts and inputs are native ratatui widgets

**Decision.** Text fields, selection prompts, dials and forms render as
ratatui widgets from pure state. Where a prompt has to work with no
full-screen UI, it has a plain-text fallback, and with no terminal at all it
fails naming the flag that would have answered it.

**Why.** One source app renders prompts as lines of text with ANSI codes and
converts them back to styled spans inside its full-screen shell. That gives
one implementation everywhere, but the conversion loses information when two
roles share a color, widths have to be measured around escape codes, and
hints end up patched with string replacement. Widgets over pure state keep
the single implementation without the round trip.

## Background helpers use threads; async is opt-in

**Decision.** `avandar-tui-tasks` is built on standard threads and channels.
Helpers for tokio live behind a feature.

**Why.** Two of the three source apps are synchronous and one is async. Parts
that never own the event loop must work in both, and threads work in both. An
app that does not use tokio must not compile it.

## One fuzzy matcher: nucleo

**Decision.** Fuzzy matching uses `nucleo-matcher`, behind the `fuzzy`
feature of the widgets crate.

**Why.** One source app carries two matchers. nucleo is the one behind the
picker with the behavior worth keeping: every query word matched as a run,
path-aware scoring, and match positions for highlighting.

## Git tags first, crates.io later

**Decision.** Apps depend on a git tag until the first building block has a
real user. Publishing to crates.io starts then, with `release-plz`.

**Why.** A crate name and a published version are permanent. Tags let the
APIs change freely while the first apps adopt them. `release-plz` publishes a
workspace's crates in dependency order and writes the changelogs from
Conventional Commits, which this repository already uses.

## CI runs the full check on three platforms

**Decision.** Every pull request runs, on GitHub Actions: formatting, clippy,
tests and docs on macOS, Linux and Windows; a build at the minimum Rust
version; and the headless xterm.js tests, with tmux on macOS and Linux.

## This repository is the authoritative source

**Decision.** Code and ideas that started in the apps the toolkit was
extracted from carry no attribution notes here: no credits in source files,
crate docs or the README. The copyright holder is Avandar Labs, Inc., as
`LICENSE` states.

**Why.** The earlier versions are the same author's own work, so there is
nobody to credit, and notes pointing elsewhere would make this repository read
as a derivative when it is meant to be the reference.

**Consequences.** This does not extend to other people's projects: ideas
taken from them are reimplemented, never copied, and their licenses are
respected.

## An agent can run every test layer

**Decision.** Every test layer can be started, driven and read by an agent,
with no person in the loop. Whoever makes a change runs the layers it touches
before calling it done. Layers that cannot run in CI are still automated and
still run, locally. What is left for a person is a short checklist of
judgments before a release. See `docs/testing.md`.

**Why.** A test that needs a person is a test that gets skipped under
pressure, and a toolkit with five first-class configurations on three
platforms has too many of them to check by hand. Automating a layer that only
runs locally still removes the manual step from every phase of work.

**Consequences.** Test environments are part of the project: a Linux
container, a macOS virtual machine for iTerm2, a browser harness for
xterm.js, and a Windows virtual machine. Reliability comes before coverage: a
test that cannot be made reliable is not automated.

## xterm.js is tested in a real browser

**Decision.** The xterm.js harness runs the browser build of xterm.js in a
headless browser and sends it real mouse and keyboard events. It does not use
the headless build of xterm.js.

**Why.** The code that turns a click or a key press into bytes exists only
in the browser build, and that encoding is exactly what a mouse and keyboard
toolkit has to be tested against. A throwaway version of the harness gave
thirty identical runs in a row, at about four seconds each, once two mistakes
in the harness itself were fixed. It runs on all three platforms, so it is
also how the Windows pseudo-console is exercised.

**Consequences.** The repository carries a small Node project for the
harness, pinned to the xterm.js version Orca ships. A live Orca terminal is
checked separately through Orca's command line, so a change in Orca's
version or settings is noticed.

**Revisit when.** The browser build and Orca's behavior are found to differ
in a way the probe cannot detect.

## iTerm2 is driven in a virtual machine, over VNC

**Decision.** iTerm2 tests run in a macOS virtual machine. Input reaches
iTerm2 as real pointer and keyboard events sent through the virtual machine's
VNC server, and results come back through a shared folder. This replaces the
earlier idea of scripting iTerm2 through its Python API on a Mac's own
desktop.

**Why.** On a real desktop, macOS asks a person to approve an app the first
time it opens and again before one program may control another, and nothing
run over SSH can see or answer those prompts. That was tried and it stalled
on the first prompt. iTerm2's Python API also cannot produce a mouse event at
all. A virtual machine gives an agent the whole screen and real input, with
nothing to approve, and keeps test windows off anyone's desktop.

**Consequences.** The virtual machine is prepared once and reused. It needs
an Apple Silicon Mac, so this layer stays local (see "iTerm2 tests run
locally, not in CI"). It runs on Tart, which is source-available and not open
source; it is a development tool and no crate depends on it.

**Revisit when.** A fully open source tool offers the same thing: a macOS
guest with input and screen exposed from outside the guest.
