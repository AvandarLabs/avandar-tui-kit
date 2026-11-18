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
