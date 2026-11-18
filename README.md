# avandar-tui-kit

A toolkit for building terminal apps with [ratatui](https://ratatui.rs).

It collects the parts that terminal apps keep rebuilding: panels and tabs,
mouse support (including selection and pass-through in embedded terminal
panes), command palettes, hotkey tables, menus, fuzzy search, and frontends
for coding agents. It wraps ratatui. It is not a new TUI framework: every part
is something your app calls, and your app keeps its own event loop and state.

> **Status: early.** The workspace is being set up and nothing is published
> yet. The plan lives in [`docs/`](docs/).

## How it is organized

This repository is a Cargo workspace. Each building block is its own crate
under `crates/`, and the `avandar-tui-kit` umbrella crate re-exports them
behind cargo features. An app turns on only what it uses, so a menu-driven app
never compiles the terminal-emulation or agent crates. See
[`docs/architecture.md`](docs/architecture.md).

## Requirements

- Rust 1.88 or newer (the floor ratatui 0.30 sets).
- [crossterm](https://crates.io/crates/crossterm) as the terminal backend.
  ratatui also runs on termion and termwiz, but the toolkit reads crossterm's
  input events and supports only crossterm for now.

## Developing

A Rust toolchain is the only prerequisite ([rustup.rs](https://rustup.rs)).
`just` is optional (`cargo install just`): every recipe is one command you can
also type by hand.

```sh
cargo test --workspace --all-features                          # just test
cargo clippy --workspace --all-targets --all-features -- -D warnings  # just lint
cargo fmt --all                                                # just fmt
just check    # everything that has to pass before a change lands
```

## Agent skills

Agent skills are not tracked in git, but `skills-lock.json` is.
`just skills-install` restores them after a clone.
Run `npx skills list` to see what is installed and
`just skills-update` to upgrade them. See
[`docs/skills.md`](docs/skills.md) for how the two skill managers
divide the work.

## Agent rules

Coding conventions live in `AGENTS.md`, which is the single source of truth.
`CLAUDE.md` (Claude Code) and `.cursor/rules/agents.mdc` (Cursor) are symlinks
to it, and it is also the file the Codex CLI reads natively. Update `AGENTS.md`
and every tool stays in sync.

## License

MIT. See [`LICENSE`](LICENSE).
