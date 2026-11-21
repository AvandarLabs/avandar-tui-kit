# Task entry points for the avandar-tui-kit workspace.
#
# `just` is a convenience, never a dependency: every recipe below is a single
# command you can also type by hand. Install it with `cargo install just`.

# Recipe arguments arrive as "$@" rather than just's own interpolation.
set positional-arguments

# Show the available tasks.
default:
    @just --list

# Compile every crate in the workspace with every feature on.
build:
    cargo build --workspace --all-features

# Run every test in every crate: the inline unit tests and each tests/.
test:
    cargo test --workspace --all-features

# Run a command inside the Linux floor container (Rust 1.88, Node, Playwright, tmux 3.3).
linux *args:
    ./rig/linux/run.sh "$@"

# Lint with clippy (pedantic + nursery are on); any warning fails.
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Lint Windows and Linux targets from the development machine (Layer 2).
lint-targets:
    cargo clippy --workspace --all-targets --all-features --target x86_64-pc-windows-msvc --target x86_64-unknown-linux-gnu -- -D warnings

# Format the source.
fmt:
    cargo fmt --all

# Fail if anything is unformatted, without rewriting it.
fmt-check:
    cargo fmt --all --check

# Build the API docs and fail on any broken intra-doc link.
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

# Everything that has to pass before a change lands (reports only: `just fmt` fixes).
check: fmt-check lint test doc

# Run an example program in the headless Playwright xterm.js harness (Layer 3).
xterm *args:
    ./rig/xterm/run.sh "$@"

# Run an example program inside a private tmux server in the xterm.js harness (Layer 4).
xterm-tmux *args:
    ./rig/xterm/run-tmux.sh "$@"

# Run an example program in an unfocused live Orca terminal (Layer 5).
orca *args:
    ./rig/orca/run.sh "$@"

# Restore the agent skills pinned in skills-lock.json (run once after cloning).
skills-install:
    npx --yes skills experimental_install

# Update every agent skill this project has.
skills-update:
    ./scripts/skills/update-skills.sh
