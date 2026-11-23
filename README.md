# avandar-tui-kit

A toolkit for building terminal apps with [ratatui](https://ratatui.rs).

It collects the parts that terminal apps keep rebuilding: panels and tabs,
mouse support (including selection and pass-through in embedded terminal
panes), command palettes, hotkey tables, menus, fuzzy search, and frontends
for coding agents. It wraps ratatui. It is not a new TUI framework: every part
is something your app calls, and your app keeps its own event loop and state.

> **Status: early, unreleased.** Mouse interaction and its laboratory are
> implemented. Windows runtime verification and human release judgments remain
> open, followed by branch review, release, and adoption. See
> [`docs/testing.md`](docs/testing.md) for the gates.

## How it is organized

This repository is a Cargo workspace. Each building block is its own crate
under `crates/`, and the `avandar-tui-kit` umbrella crate re-exports them
behind cargo features. An app turns on only what it uses, so a menu-driven app
never compiles the terminal-emulation or agent crates. See
[`docs/architecture.md`](docs/architecture.md).

The implemented [`ratatui-mouse`](crates/ratatui-mouse/README.md) crate supplies
hit areas, text selection over app-held rows and optional vt100 history, timed
edge scrolling, keyboard select mode, and pane input routing. Enable the
umbrella's `mouse` feature for `avandar_tui_kit::mouse`; no feature is on by
default, and the optional `vt100` adapter is enabled separately on
`ratatui-mouse`. The app owns its loop, focus, clipboard, children, and cleanup.
The crate README covers integration, surface rules, keyboard equivalents, and
terminal limits, including native versus in-app selection and tmux modifiers.

## Requirements

- Rust 1.88 or newer (the floor ratatui 0.30 sets).
- [crossterm](https://crates.io/crates/crossterm) as the terminal backend.
  It is the only backend the toolkit supports.

## Supported terminals and platforms

- **Terminals:** iTerm2 and xterm.js (the emulator inside Orca and VS Code).
- **Multiplexer:** tmux 3.3 or newer, inside either terminal.
- **Platforms:** macOS, Linux and Windows.

All of these are first-class: every part is tested in each. Other terminals
should work but are not tested. See [`docs/terminals.md`](docs/terminals.md).

## Developing

A Rust toolchain is the only prerequisite for building and unit-testing the
workspace ([rustup.rs](https://rustup.rs)). `just` is optional
(`cargo install just`): every recipe is one command you can also type by hand.

```sh
cargo test --workspace --all-features                                 # just test
cargo clippy --workspace --all-targets --all-features -- -D warnings  # just lint
cargo fmt --all                                                       # just fmt
just check                                                            # fmt-check, lint, test, doc
just lint-targets                                                     # Layer 2: Windows + Linux clippy
just linux just check                                                 # Layer 1: Rust 1.88 + Linux floor container
just xterm capability_probe                                           # Layer 3: headless xterm.js harness
just xterm-tmux full capability_probe                                 # Layer 4: stock or full tmux in xterm.js
just orca capability_probe                                            # Layer 5: unfocused live Orca terminal
just iterm2 capability_probe                                          # Layer 6: iTerm2 in the Tart + VNC macOS VM
just windows                                                          # Layer 7: inspect and download Windows CI logs
just xterm mouse_lab mouse                                            # X1-X14: mouse outcomes in xterm.js
just xterm-tmux stock mouse_lab mouse                                 # Same suite through stock tmux
just orca mouse_lab -- --query-modes                                  # Live keyboard Copy and lifecycle
just iterm2 mouse_lab mouse -- --query-modes                           # iTerm2 mouse matrix in the VM
```

See [`docs/testing.md`](docs/testing.md) for what each test layer proves and
where it runs.

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
