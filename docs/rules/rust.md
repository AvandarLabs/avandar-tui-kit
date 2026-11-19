# Rust rules

How Rust is written in this repository. The lint configuration in the root
`Cargo.toml` enforces the first two mechanically; the rest is on you and on
review.

## No `unsafe`

`[lints.rust] unsafe_code = "forbid"` is set, and it stays set. A problem that
seems to need `unsafe` is a problem that needs a different crate or a different
design. Raise it before reaching for the escape hatch.

## Keep clippy clean

`pedantic` and `nursery` are on at `warn` in `[workspace.lints]`, every crate
opts in with `[lints] workspace = true`, and `just lint`
(`cargo clippy --workspace --all-targets --all-features -- -D warnings`) turns
every warning into a failure. Do not land new warnings, and do not silence one
with a bare `#[allow(...)]`. When a lint is genuinely wrong for this codebase,
allow it in `[workspace.lints]` in the root `Cargo.toml` with a comment saying
why, so the exception is visible in one place instead of scattered through the
source.

## Split the pure from the impure

Decision logic goes in a pure function that takes values and returns values:
which key means what, how a line is formatted, what a parsed argument implies.
Those functions get the tests. The shells that touch the terminal, the
filesystem, the network, or a child process stay thin enough to read in one
screen, and hold no decisions worth asserting. When a shell grows an `if` that
matters, that `if` belongs in a pure function next to it.

Write tests red first: a failing test that names the behavior, then the
smallest code that turns it green, then the cleanup with the bar green.

## Write for three platforms

macOS, Linux and Windows are all first-class. Do not reach for a Unix-only
API (`/dev/tty`, `std::os::unix`, process groups, signals, a shell) without
giving Windows the same behavior behind the same public API. Keep the
platform split in one small module per concern, chosen with `#[cfg(...)]`, so
the rest of the crate has no platform branches. Never shell out to a tool
that exists on one platform only without a counterpart for the others. A test
that can only run on one platform says so with `#[cfg(...)]` and has a sibling
for the others where the behavior exists there.

## Errors are typed in libraries

Every crate here is a library, so a fallible public function returns a
concrete error type the caller can match on: an enum per crate, or per module
when a crate has unrelated failure families, with a `Display` that reads as one
sentence and a `source()` that keeps the cause. Do not return `anyhow::Error`
from a public API: it forces the app to string-match to tell failures apart.
Examples and tests may use `anyhow` freely. Document every fallible public
function with an `# Errors` section and every function that can panic with a
`# Panics` section.

## One module per file, and keep files small

Prefer many small single-responsibility modules over a few large ones. A `.rs`
file that passes roughly **400 lines of production code** is a smell: split it
into a directory of submodules (`foo.rs` becomes `foo/mod.rs` plus
`foo/<part>.rs`), and let the directory tree mirror the ownership in the code.
Inline `#[cfg(test)]` blocks do not count toward that budget, but they are held
to the same standard: split a large or multi-purpose suite by behavior, and
build fixtures from small composable helpers rather than one catch-all harness.
`mod.rs` stays a thin glue and re-export layer. Split along a real seam, never
at an arbitrary line, and do not split a file that is already cohesive just to
hit a number.

## Comment the why, not the what

The function name and its doc comment cover what it does. A comment earns its
place when the reason is not obvious from the code: a surprising API, an
ordering that matters, a workaround and the thing it works around. Document
every exported item with `///`.

## Do not add dependencies casually

Every dependency a crate takes is one an app compiles and audits, and the
toolkit promises that an app pays only for what it turns on. Before adding
one, check whether the standard library, ratatui, crossterm, or a crate
already in the workspace does the job. When it is worth it, declare its
version once in `[workspace.dependencies]` with a comment saying what it is
for, take it in the crate with `.workspace = true`, and put it behind a cargo
feature when only part of the crate needs it. Record it in the crate's section
of `docs/architecture.md`.

## `docs/` is the durable record

Code says what the program does today; `docs/` says how it fits together and
why. Update the affected document in the same change that changes behavior.
Design rationale belongs in `docs/decisions.md`: one section per decision, what
was chosen, what it rules out, and what would make it worth revisiting.

## Versions move with releases

The version lives once, in `[workspace.package]` in the root `Cargo.toml`, and
every crate inherits it. Ordinary changes do not bump it: a release does, with
the bump chosen by the semver rules below. Before
1.0, a breaking change to a public API bumps the minor version and anything
else bumps the patch version. `Cargo.lock` is committed so every contributor
and CI run builds the same dependency graph.
