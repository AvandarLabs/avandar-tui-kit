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

# Lint with clippy (pedantic + nursery are on); any warning fails.
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

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

# Restore the agent skills pinned in skills-lock.json (run once after cloning).
skills-install:
    npx --yes skills experimental_install

# Update every agent skill this project has.
skills-update:
    ./scripts/skills/update-skills.sh
