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
├── rules/              how code is written here
└── skills.md           how the agent skills are installed
```

## One workspace, many crates, one umbrella

Each building block is its own crate under `crates/`. The `avandar-tui-kit`
crate owns no code: it re-exports each building block behind a cargo feature
of the same name, with nothing on by default. An app depends on the umbrella
and turns on the features it wants, or depends on a single building block
directly.

The split exists so an app pays only for what it uses. A menu-driven app with
no embedded terminal must never compile a terminal emulator, a pseudo-terminal
library, or the agent layer. Splitting by crate rather than by feature inside
one crate keeps each part's dependencies, tests, and documentation separate,
and lets a generally useful part (mouse support for ratatui) carry a name that
does not tie it to this toolkit.

The crate map is not settled yet. It follows the
[inventory](inventory/README.md) of UI elements in the source apps, which
proposes a split along the heavy dependencies, and it will be fixed in the
architecture spec.

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

## Backend

crossterm is the only supported backend for now. ratatui can also draw through
termion or termwiz, but every source app uses crossterm and the input-handling
parts read crossterm's event types. Supporting another backend later means
putting input types behind a feature, not rewriting the parts.

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
