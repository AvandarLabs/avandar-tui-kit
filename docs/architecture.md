# Architecture

avandar-tui-kit is a cargo workspace of library crates for terminal apps built
with ratatui. Extend this document as the shape changes: it is meant to grow,
not to be replaced.

## The layout

```
.github/
└── workflows/          CI across Linux, macOS, and Windows (ci.yml)
Cargo.toml              the workspace: members, shared package settings,
                        shared dependency versions, shared lints
justfile                one-command entry points for every check and test layer
crates/
├── avandar-tui-kit/    the umbrella crate: re-exports the others behind features
└── ratatui-mouse/      reporting, selection, keyboard select mode, pane routing
docs/
├── archive/            completed specs, moved from specs/ for reference
├── architecture.md     this file
├── code-walkthroughs/  dated code walkthroughs copied from diff-review
├── decisions.md        one section per decision worth remembering
├── inventory/          every extractable UI element in the source apps
├── milestones.md       the order the crates are built in, and what ends each
├── mouse.md            mouse integration, surfaces, routing, and laboratory contract
├── plans/              in-progress or queued implementation plans (deleted on completion)
├── specs/              unimplemented designs waiting to be built
├── terminals.md        the first-class terminals and platforms, and what each does
├── testing.md          the test layers, where each runs, and the rules for them
├── rules/              how code is written here
└── skills.md           how the agent skills are installed
rig/
├── expected/           normalized capability baselines for all 13 configurations
├── iterm2/             Layer 6 Tart + VNC macOS virtual machine driver
├── linux/              Linux floor container (Rust 1.88, Node, Chromium, tmux 3.3a)
├── orca/               Layer 5 unfocused live Orca terminal runner
├── probe/              unpublished capability_probe crate (moves to avandar-tui-term in M3)
├── tmux/               stock and full tmux server configurations
├── windows/            Layer 7 CI Windows log and artifact fetcher
└── xterm/              Layers 3 and 4 Playwright + xterm.js + node-pty harness
scripts/
├── ci/                 workspace feature-matrix checks
└── skills/             agent skill installation and update helpers
```

## One workspace, many crates, one umbrella

Each building block is its own crate under `crates/`. The `avandar-tui-kit`
crate owns no code: it re-exports each building block behind a cargo feature
named for the block (`mouse`, `term`, `pty`, `agent`, `widgets`, `document`,
`charts`, `tasks`), with nothing on by default. An app depends on the umbrella
and turns on the features it wants, or depends on a single building block
directly.

The split exists so an app pays only for what it uses. A menu-driven app with
no embedded terminal must never compile a terminal emulator, a pseudo-terminal
library, or the agent layer. Splitting by crate rather than by feature inside
one crate keeps each part's dependencies, tests, and documentation separate,
and lets a generally useful part (mouse support for ratatui) carry a name that
does not tie it to this toolkit.

The crate map follows the [inventory](inventory/README.md) of UI elements in
the source apps and splits along the heavy dependencies:

| Crate | Owns | Heavy dependencies |
| --- | --- | --- |
| `ratatui-mouse` | Mouse modes, hit-testing, clicks, selection, wheel routing, pass-through | vt100, behind a feature |
| `avandar-tui-term` | Borrowing the terminal and giving it back, keyboard flags, clipboard | none |
| `avandar-tui-pty` | Embedded terminal panes, key encoding, child supervision | portable-pty, vt100, tui-term |
| `avandar-tui-agent` | Coding-agent frontends: launch, resume, typing and submitting prompts | the `pty` crate, behind a feature |
| `avandar-tui-widgets` | Style, layout, command palette and shortcuts, pickers, input and forms | nucleo behind `fuzzy`, tui-tree-widget behind `tree`, walkdir behind `tree-walk`, clap behind `clap`, jiff behind `jiff` |
| `avandar-tui-document` | Opening, viewing and (in M9.b) opt-in editing of documents: markdown, scrollable documents, live-tail logs, target opening, nano/vim editing | pulldown-cmark behind `markdown`, open; upstream editor crate if chosen in M9.a |
| `avandar-tui-charts` | Text charts and tables | none |
| `avandar-tui-tasks` | Background work a UI needs | process-wrap behind `process`, tokio behind `tokio` |

The crates are created as their specs are written, not ahead of them.
[`specs/architecture.md`](specs/architecture.md) is the design they are built
from: each crate's modules and features, which inventory element lands where
and which are left out, the conventions every public API follows, and the
platform, release and testing policies. The first per-crate spec is
[`specs/ratatui-mouse.md`](specs/ratatui-mouse.md).

Three things are shared across crates, and each has one home:

| Shared | Home |
| --- | --- |
| Theme roles, the cell-width function, wrapping and truncation | The `style` module of `avandar-tui-widgets`. The document, charts and pty crates depend on widgets for it, and with no feature on widgets pulls in nothing heavy. |
| Hit areas for the mouse | `ratatui-mouse`. A widget records plain rectangles, and registers them when its `mouse` feature is on. |
| Events, rectangles, colors and styles | crossterm's and ratatui's own types. `ratatui-mouse` needs no other crate of the workspace, and it and `avandar-tui-term` speak only in those types. |

Where two blocks only meet in some apps (mouse support in the terminal guard,
the pane as an agent's transport, selection inside a pane), the link is a
feature, and the umbrella turns it on by itself when both blocks are on.

## Shared settings

The root `Cargo.toml` holds, once, everything the crates must agree on:

| Section | What it fixes |
| --- | --- |
| `[workspace.package]` | Version, edition 2024, minimum Rust version, MIT license, repository |
| `[workspace.dependencies]` | One version of ratatui, crossterm, and every other shared dependency |
| `[workspace.lints]` | `unsafe_code` forbidden, `missing_docs` warned, clippy pedantic and nursery on |

A crate inherits each with `<key>.workspace = true` and adds only the features
it needs. Two crates disagreeing on a ratatui version would make an app
compile two copies and fail to pass types between them, which is why versions
are never written in a member crate.

## Backend and target terminals

crossterm is the only supported backend. ratatui can also draw through
termion, termwiz or termina, but a backend is only the library that talks to
the terminal: it decides nothing about which terminals an app works in. Public
APIs use crossterm's event types directly, with no abstraction over backends.

What an app works in is decided by which escape-sequence features the
terminal in front of it implements. The first-class targets are **iTerm2** and
**xterm.js** (the emulator inside Orca and VS Code), **tmux** inside either,
and **macOS, Linux and Windows**. Every crate that writes to or reads from the
terminal is tested in all of them. See [`terminals.md`](terminals.md) for
what each one does and [`testing.md`](testing.md) for how it is proved.

## Dependencies

| Crate | Why it is here |
| --- | --- |
| `ratatui` | The rendering library every part builds on. Declared without default features so each crate turns on only what it uses. |
| `crossterm` | The terminal backend and input event types. Pinned to the version ratatui 0.30 builds against. |
| `unicode-segmentation` | Grapheme boundaries for mouse selection over app-held text; shared with ratatui's dependency tree. |
| `vt100` | Optional emulator-backed mouse surfaces, enabled only by the mouse crate's `vt100` feature. |

Adding to this table means saying what the crate is for, in one line, in the
same change. See `docs/rules/rust.md`.

## ratatui-mouse

The [mouse building block](mouse.md) combines reporting lifecycle, frame hit
areas, click counting, content coordinates, borrowed text/emulator surfaces,
selection and highlights, pane routing/encoding, and keyboard select mode.
`MouseController` owns gesture and deadline state, while the app owns its loop,
clock, drawing stream, focus, clipboard, and child I/O. The crate root
re-exports the public interfaces; the optional umbrella `mouse` feature exposes
them without enabling `vt100`.

`modes/platform.rs` is the only place that writes and flushes the supplied
drawing stream on macOS and Linux or invokes crossterm's console operations
on Windows. The guard handles cleanup and failures without platform branches;
the app keeps ownership of its event loop and panic hook. Surface lookups borrow
app state only for a controller call; ticks use injected time and expose the
next deadline, without a background task. No crate component queries terminal
modes. The laboratory alone queries, logs, and performs terminal I/O for the
test rigs. Implementation is present; Windows runtime and subjective release
verification remain open as recorded in [testing](testing.md).

| Dependency | Mouse crate use |
| --- | --- |
| `ratatui` | Shared rectangles, positions, buffers, styles, and cell width, with default features disabled. |
| `crossterm` | Windows mouse console setup and restoration, and shared event types. |
| `unicode-segmentation` | Grapheme boundaries for app-held-text selection. |
| `vt100` | Borrowed emulator-backed surfaces behind feature `vt100`; no default feature. |

## Public READMEs and crate docs

Published crate READMEs, `Cargo.toml` metadata, and rustdoc headers follow two
rules:

- **Be upfront about wrappers.** When a crate or module is a light wrapper or
  glue layer over existing ecosystem crates (`portable-pty`, `vt100`, and
  `tui-term` in `avandar-tui-pty`, `tui-tree-widget` or `nucleo-matcher` in
  `avandar-tui-widgets`, `pulldown-cmark`, `open`, or an upstream editor crate
  in `avandar-tui-document`), say so plainly at the top, state the specific
  functionality this crate adds, and tell visitors when they are better off
  using the base crate directly.
- **Never leak personal source workflows.** Internal source apps (`brain`,
  `huckleberry-cli`, and similar personal tools) are recorded only in internal
  planning docs under `docs/`, never in any README, `Cargo.toml`, or rustdoc
  published to crates.io or shown as a crate entrypoint.

## Where the parts come from

The toolkit extracts patterns that already exist in these public apps, and
reimplements them as generic parts:

| App | Repository | What it is |
| --- | --- | --- |
| `dif` | `AvandarLabs/avandar`, under `agent-skills/public-skills/skills/diff-review/scripts/dif/` | A two-pane review shell: diff server log and markdown views on the left, a coding agent in an embedded terminal on the right |
| `brain` | `jpsyx/brain` | A persistent shell: tasks, fuzzy search, file tree and logs on the left, agent sessions in tabs on the right |
| `huckleberry-cli` | `jpsyx/huckleberry-cli` | A menu-driven shell with dashboards, forms and pickers. No embedded terminal. |
| `quickstarter` | `AvandarLabs/quickstarter` | Its `rust-tui` template carries a command palette, shortcuts table and pure keymap |

### Mouse laboratory dependencies

The optional umbrella `mouse` feature re-exports `ratatui-mouse` through an
optional path-and-version dependency. `mouse_lab` is a standalone example,
requiring the mouse crate's `vt100` feature. Its JSON settings and flushed JSONL
protocol use `serde` and `serde_json` as dev-dependencies, with versions declared
once in the workspace. A dev-only ratatui dependency enables the crossterm
backend for drawing. Normal ratatui edges still disable default features, and
the public library types acquire no serialization derives.

The example's Unix-only dev-dependency on `rustix` supplies safe nonblocking
terminal reads and readiness polling for bounded DECRPM mode queries.
Crossterm's public event parser does not expose these replies. A synchronous
bounded reader finishes before crossterm consumes input and leaves no blocked
thread behind. Windows uses the console backend and records expected reply
absence. The [mouse documentation](mouse.md) defines the observable lab
fixtures, event loop, logs, and readiness handshake used by the terminal rigs.
