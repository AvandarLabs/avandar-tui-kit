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
`avandar-tui-widgets`, `avandar-tui-document`, `avandar-tui-charts` and
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
container, a macOS virtual machine for iTerm2, and a browser harness for
xterm.js. Reliability comes before coverage: a
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

## Windows is tested on CI runners only

**Decision.** Windows tests run on GitHub's hosted Windows runners and
nowhere else. There is no local Windows machine or virtual machine. Locally,
the Windows targets are linted from a Mac before every push.

**Why.** Windows needs a paid license to be used properly in a virtual
machine, and no always-on Windows machine is available to connect to. Hosted
runners are free for public repositories, already licensed, and run the same
tests on every pull request anyway.

**Consequences.** The Windows loop is push, wait, read the logs, so it is
slower than every other platform's. Windows-only code stays small and
isolated, and the Windows jobs upload the logs a failure needs. An agent
working on Windows behavior needs permission to push its branch.

**Revisit when.** A licensed Windows machine or image becomes available, or
the slow loop starts costing more than a license would.

## The theme asks for the background color and has a palette for no answer

**Decision.** A theme never assumes a dark terminal. The app asks the
terminal for its background color once at startup, and the theme picks a dark
or a light palette from the answer. The wait is bounded (200 ms), and when
nothing answers the theme uses a third palette that reads on both: the
default foreground, bold, dim and reversed, and the few colors legible on
either background. `NO_COLOR` selects a palette with no color at all. The
rules are in the theme section of `docs/specs/architecture.md`.

**Why.** The earlier rule assumed dark because terminals were thought not to
say. Measured, every first-class terminal answers the query, and a fresh
iTerm2 is light, so the assumption was wrong on a default install. The answer
can still be late or missing (tmux inside iTerm2, the Windows pseudo-console),
and a palette that is readable either way is better than a guess that is
unreadable half the time.

**Consequences.** The query lives in `avandar-tui-term` and the palettes in
the widgets crate's `style` module, joined by the app through a ratatui
color. The reply is parsed from crossterm's events, which is the only way to
read it on Windows, and a late reply is swallowed so it is never typed into
the app. `docs/rules/tui.md` was changed to match.

**Revisit when.** A first-class terminal stops answering the query.

## One width function, and app text is made portable before it is drawn

**Decision.** The workspace measures text with one function: ratatui's own
cell width, summed over grapheme clusters. Text an app hands to a part is
rewritten before it is drawn so that iTerm2, xterm.js and tmux agree on its
width: a variation selector and a skin tone are dropped, a ZWJ sequence that
starts with a one-cell character is cut to that character, and a zero-width
cluster is removed. What is stored or copied is never rewritten. The policy
is in the text width section of `docs/specs/architecture.md`.

**Why.** The three terminals disagree on exactly those sequences, and no
measuring function can agree with all of them. Measuring the way ratatui
places text keeps layout and drawing from ever differing, and drawing only
what all three measure the same keeps columns aligned and cells from going
stale. A width table per terminal was set aside: tmux in front changes the
answer, an SSH hop hides the terminal, and the table goes stale.

**Consequences.** A few emoji look plainer than their source. Nothing in the
workspace calls `unicode-width`, `Span::width` or `Line::width`, or counts
characters, for layout.

**Revisit when.** The three terminals converge, or an app needs the original
sequences drawn and accepts the misalignment.

## Shared primitives live in the widgets crate's style module

**Decision.** Theme roles, the width function, wrapping, truncation and
column fitting live in `avandar_tui_widgets::style`. The document, charts and
pty crates depend on the widgets crate for them. `ratatui-mouse` and
`avandar-tui-term` take and return ratatui's own types instead, and depend on
neither. Hit areas live in `ratatui-mouse`.

**Why.** It keeps the eight crates decided earlier. With no feature on, the
widgets crate is plain Rust over ratatui, so depending on it costs compile
time and no dependency.

**Consequences.** `style` imports nothing from its sibling modules, and a
test holds it to that, so it can become a crate of its own by moving a
directory and re-exporting it from the same path.

**Revisit when.** The widgets crate with no features gains a dependency, a
crate outside those three needs a role or a width, or a charts-only app's
build is measurably slowed. Then `style` becomes a ninth crate.

## Mouse reporting on Windows goes through the console

**Decision.** On macOS and Linux `ratatui-mouse` writes its own mode sequence
(1000, 1002, 1006). On Windows it writes no sequence and turns mouse input on
in the console's input mode through crossterm.

**Why.** crossterm on Windows reads the console's input records, not bytes,
and mouse events arrive only when the console's input mode includes them.
crossterm is the maintained, safe way to set that mode, and `unsafe` is
forbidden here. Which reports the pseudo-console then asks the terminal for
is its own choice.

**Consequences.** The rule against crossterm's `EnableMouseCapture` is about
the sequence it writes on Unix. If the pseudo-console asks the terminal for
every pointer move, hover reaches the app on Windows, the crate ignores it,
and the docs say so.

**Revisit when.** crossterm reads terminal input as bytes on Windows, which
would let the same sequence be used everywhere.

## Select mode makes selection reachable without a modifier or a mouse

**Decision.** `ratatui-mouse` has a select mode: selection driven from the
keyboard on one pane or view, during which the mouse there always selects and
is never passed through. An app that embeds a pane must offer a command that
starts it.

**Why.** Shift selects instead of passing through, but xterm.js on Linux and
Windows keeps Shift for its own selection, so the app never sees it there.
Stock tmux forwards no mouse events at all. Select mode is the one route to
selection that works in every first-class configuration, and it is the
keyboard equivalent the "keyboard first" rule already requires.

**Revisit when.** Never for the keyboard path. A second modifier for the
mouse is an open question in the mouse spec.

## One version and one tag for every crate

**Decision.** Every crate, `ratatui-mouse` included, carries the workspace's
one version, and a release is one `vMAJOR.MINOR.PATCH` tag that covers all of
them. A breaking change in any crate bumps the minor version of all.

**Why.** An app has to take every toolkit crate from the same tag, or it
compiles two copies whose types do not pass between each other. One version
makes that the only thing an app can do.

**Revisit when.** Publishing to crates.io starts and a crate has users
outside the toolkit. `release-plz` can version crates apart.

## Error types are written by hand

**Decision.** Each crate's error enum implements `Display` and
`std::error::Error` itself. No error-derive crate is a dependency.

**Why.** The enums are small, and a derive macro is a dependency every app
would compile for a dozen lines per crate.

**Revisit when.** An error enum grows past what is pleasant to write by hand.

## Alt joins Shift as a selection modifier, once measured

**Decision.** In a pane whose program has the mouse, Alt held at a press
selects text, in addition to Shift, on every platform. It becomes a default
only after the xterm.js harness shows, on Linux and Windows, that Alt with a
click and Alt with a drag reach the app. Until then the default stays Shift
alone. Select mode, the keyboard-reachable way to select, is required of every
app that embeds a pane regardless.

**Why.** xterm.js on Linux and Windows reserves Shift for its own selection,
so the app never sees Shift with a click there, while Shift is the settled
modifier. Select mode already covers that case. Alt gives the same one-gesture
path on every platform, and on macOS Option with a click is measured to arrive.

**Consequences.** A pane's program never receives Alt with the mouse. The
`selection_modifiers` default in `ratatui-mouse` changes from Shift to Shift
and Alt when the measurement passes, which is part of milestone 0's exit
criteria.

**Revisit when.** The measurement shows Alt does not arrive on some platform.

## crates.io publishing starts in milestone 2

**Decision.** Publishing moves from git tags to crates.io in milestone 2,
right after `dif` adopts `ratatui-mouse`, as `docs/milestones.md` plans.

**Why.** This is the earlier "git tags first" decision applied: the first
crate then has a real user, and the next crates are written against a
published, versioned `ratatui-mouse`. The API has to be settled by then,
because a published name and version are permanent.

**Revisit when.** `dif`'s adoption shows the API still needs breaking changes.

## `avandar-tui-document` starts with read-only parity and grows into opt-in editing

**Decision.** The document crate is named `avandar-tui-document` (re-exported
as `document` in the umbrella), covering documents opened in any of the
toolkit's forms: markdown, scrollable documents with or without a cursor,
live-followed logs, and cursor-targeted file or URL opening. It is built in two
milestones: M9.a delivers read-only parity for the source apps and surveys
existing ratatui document-viewer and text-editor crates up front so its types
and settings leave room for editing; M9.b adds opt-in file editing with
caller-chosen editor styles (nano-style modeless chords and vim-style normal
and insert modes with common cursor motions), wrapping an existing ratatui
crate when one fits.

**Why.** `avandar-tui-view` sounded like a generic layout container or base
view trait rather than a document opener and reader. Keeping M9.a strictly at
read-only parity moves the source apps onto the crate without waiting on an
editor, while doing the editor research in M9.a keeps M9.b from forcing a
breaking redesign of the document state and options.

**Consequences.** Callers pass options controlling whether editing is allowed
(read-only by default) and which editor style (`nano` or `vim`) is active when
it is. M9.b does not block M10 to M13 because none of the source apps need
in-place file editing for parity, and placing M9.b after M10 lets vim-style
editing share `command::motion` where it fits.

**Revisit when.** M9.a's ecosystem survey finds an upstream crate whose buffer
model changes the split between the read-only and editable paths.

## Published READMEs and crate docs are upfront about wrappers and never leak personal workflows

**Decision.** Every published README and rustdoc crate header states plainly
when a crate or module is a thin wrapper or glue layer over upstream crates,
names those base crates, explains the specific behavior our crate adds, and
recommends depending on the base crate directly when a visitor does not need
that extra layer. Published READMEs, rustdoc comments, and `Cargo.toml`
metadata never mention personal or internal source apps (`brain`,
`huckleberry-cli`, and similar private workflows) that inspired the toolkit.

**Why.** Visitors evaluating a dependency deserve an honest description of what
the crate actually builds rather than salesmanship, and wrapping a solid
ecosystem crate is welcome when credited clearly. At the same time, honesty is
about technical scope, not exposing personal workflows or private repositories
on crates.io and docs.rs.

**Consequences.** Milestone 2 and every subsequent crate milestone verify both
rules before release: wrapped crates are credited with guidance on when to use
them directly, and personal app names stay strictly inside repository planning
docs under `docs/`, never in published READMEs, `Cargo.toml` fields, or
rustdoc.

**Revisit when.** Never.

## The capability probe lives in the test rig until avandar-tui-term exists

**Decision.** During Milestone 0, the capability probe and its pure VT reply
parsers live in an unpublished workspace member crate at `rig/probe`
(`publish = false`), with `capability_probe` as an example binary driven by
every test layer. It moves into `avandar-tui-term` in Milestone 3.

**Why.** Milestone 0 measures every first-class terminal, `tmux` mode, and
Windows ConPTY behavior before any library crate is written, so the specs and
`ratatui-mouse` rest on measured facts rather than assumptions. Creating
`avandar-tui-term` ahead of its spec just to hold the probe would violate the
spec-first order in `docs/milestones.md`.

**Consequences.** `rig/probe` inherits `[workspace.package]` and
`[workspace.lints]` like every member crate, stays unpublished
(`publish = false`), and owns the Windows `.cmd` shim proof (`cmd_shim`)
alongside the reply parsers. When Milestone 3 builds `avandar-tui-term`,
`capability_probe` is rewritten over that crate's modules and `rig/probe`
leaves the workspace.

**Revisit when.** Milestone 3 lands `avandar-tui-term` and moves
`capability_probe` into its permanent home.

## tmux 3.3 is tested in the Linux floor container

**Decision.** `tmux 3.3` (`3.3a`, Debian Bookworm's package), the minimum
supported `tmux` version, is pinned and tested inside `rig/linux` alongside
`rustc 1.88.0`, while macOS runs test current Homebrew `tmux` (`3.8`).

**Why.** `tmux 3.3` is the oldest release with `allow-passthrough`, which sets
the toolkit's floor, and its behavior differs from `tmux 3.8` in ways only real
execution exposes (`tmux 3.3a` defaults `mouse` to `off`, leaves `DECRQM` and
`CSI 16 t` unanswered, does not forward `OSC 11 ; ?` background queries, and
uses older Unicode width tables). Putting `tmux 3.3a` in the same Debian
Bookworm container that already pins Rust `1.88` tests both floors in one
reproducible image locally (`just linux`) and in CI without building a second
`tmux` on macOS.

**Consequences.** `rig/expected` carries separate macOS (`tmux 3.8`) and Linux
(`tmux 3.3a`) baselines (`xterm-tmux-{stock,full}-{macos,linux}.json`), and
`docs/terminals.md` records both versions wherever they differ. Every crate
touching terminal I/O proves it degrades cleanly when `DECRQM` and `OSC 11 ; ?`
go unanswered in `tmux 3.3a`.

**Revisit when.** The minimum supported `tmux` version is raised, or the Linux
floor container moves off Debian Bookworm and needs `tmux 3.3a` built from
source to keep testing that floor.
