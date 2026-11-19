# Architecture

avandar-tui-kit is a cargo workspace of library crates for terminal apps built
with ratatui. Extend this document as the shape changes: it is meant to grow,
not to be replaced.

## The layout

```
Cargo.toml              the workspace: members, shared package settings,
                        shared dependency versions, shared lints
crates/
└── avandar-tui-kit/    the umbrella crate: re-exports the others behind features
docs/
├── architecture.md     this file
├── decisions.md        one section per decision worth remembering
├── inventory/          every extractable UI element in the source apps
├── milestones.md       the order the crates are built in, and what ends each
├── specs/              the designs the crates are built from
├── terminals.md        the first-class terminals and platforms, and what each does
├── testing.md          the test layers, where each runs, and the rules for them
├── rules/              how code is written here
└── skills.md           how the agent skills are installed
```

## One workspace, many crates, one umbrella

Each building block is its own crate under `crates/`. The `avandar-tui-kit`
crate owns no code: it re-exports each building block behind a cargo feature
named for the block (`mouse`, `term`, `pty`, `agent`, `widgets`, `view`,
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
| `avandar-tui-view` | Read-only viewers: markdown, documents with a cursor, logs | pulldown-cmark behind `markdown` |
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
| Theme roles, the cell-width function, wrapping and truncation | The `style` module of `avandar-tui-widgets`. The view, charts and pty crates depend on widgets for it, and with no feature on widgets pulls in nothing heavy. |
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

Adding to this table means saying what the crate is for, in one line, in the
same change. See `docs/rules/rust.md`.

## Where the parts come from

The toolkit extracts patterns that already exist in these public apps, and
reimplements them as generic parts:

| App | Repository | What it is |
| --- | --- | --- |
| `dif` | `AvandarLabs/avandar`, under `agent-skills/public-skills/skills/diff-review/scripts/dif/` | A two-pane review shell: diff server log and markdown views on the left, a coding agent in an embedded terminal on the right |
| `brain` | `jpsyx/brain` | A persistent shell: tasks, fuzzy search, file tree and logs on the left, agent sessions in tabs on the right |
| `huckleberry-cli` | `jpsyx/huckleberry-cli` | A menu-driven shell with dashboards, forms and pickers. No embedded terminal. |
| `quickstarter` | `AvandarLabs/quickstarter` | Its `rust-tui` template carries a command palette, shortcuts table and pure keymap |
