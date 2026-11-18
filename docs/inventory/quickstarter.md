# Inventory: quickstarter

`quickstarter` scaffolds new projects. Its `rust-tui` capability generates a
small terminal UI, and that template is the cleanest statement of the rules
this toolkit follows. This repository was itself generated from it, and its
first commit holds an identical copy of the template's `src/tui/`.

- **Repository:** `AvandarLabs/quickstarter`. Paths below are repo-relative.
- **Versions:** ratatui 0.30, crossterm 0.29.
- **Shape:** pure functions for every decision (a key classifier, a single
  `update(state, action)`, a palette model), a pure renderer, and a thin loop.
  The UI draws on stderr.

## Elements

### Command vocabulary and catalog `cmd`
One enum of every command, one ordered catalog, one label each. The palette
shows the whole catalog.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/command.rs` (about
  95)
- **Effort:** low. **Tests:** yes.

### Pure key classifier by open overlay `cmd`
Turns a key and the open overlay into an action. What a key means depends on
what is open: Ctrl+P opens the palette from the main view and moves the
selection inside it; a printable character is a binding on the main view and
a letter of the query in the palette.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/keymap.rs` (about
  290 with tests)
- **Effort:** low. **Tests:** yes, a case per binding.

### Palette with word-atom filtering `cmd`
Every whitespace-separated query word must appear in the row, in any order,
so "open help" and "help open" find the same row.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/palette.rs` (about
  340 with tests)
- **Effort:** low. **Tests:** yes.

### Shortcuts table with parity tests `cmd`
Each shortcut names the commands it runs. One test fails when a shortcut runs
a command the palette does not list; a second pins the short list of
navigation-only bindings that run no command. A third checks that a hint
printed on a palette row names a real binding.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/shortcuts.rs`
  (about 240 with tests)
- **Effort:** low. **Tests:** yes. brain carries the same guards.

### State and a single update function `layout`
The whole UI state plus `update(state, action) -> state`, with no terminal or
clock, so whole sessions are played as tests.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/state.rs` (about
  260)
- **Effort:** pattern. **Tests:** yes.

### Pure renderer and centered rect `layout`
Draws from state only; a `centered` helper and a check of whether the theme
paints at all.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/render.rs` (about
  325)
- **Effort:** low. **Tests:** partial.

### Terminal guard and panic hook `term`
A guard that restores on drop plus a panic hook that restores before the
message prints. The only source that has both.
- **Where:** `templates/capabilities/rust-tui/files/src/tui/mod.rs` (about 105)
- **Effort:** low. **Tests:** none, by design.

### Semantic theme `style`
Nine roles (heading, accent, value, muted, success, warning, error, info,
prompt), each one bright ANSI color, gated on stderr being a terminal and
`NO_COLOR` unset. A test keeps every role in the bright half of the palette.
- **Where:** `templates/project-types/rust-cli/files/src/theme.rs` (about 250)
- **Effort:** low. **Tests:** yes. huckleberry-cli's theme is a superset.

### Question model `input`
A question names the value and the flag that answers it; a pure interpreter
handles typed answers; with no terminal the failure names the flag.
- **Where:** `templates/project-types/rust-cli/files/src/prompt.rs` (about 290)
- **Effort:** low. **Tests:** yes. huckleberry-cli has a richer version.

### Arrow-key selection prompts (marginal) `pick`
Single select, multi-select and confirm built on dialoguer. Every enum-shaped
answer is a list, never free text.
- **Where:** `cli/src/cli/prompts.rs` (about 230)
- **Note:** not ratatui; the toolkit's selection prompt would replace it.

## Rules it states

`templates/capabilities/rust-tui/files/docs/rules/tui.md`: the palette is the
parent set of every command, rendering is a pure function of the state, key
handling is a classifier and never an `if` in the loop, and the terminal is
restored on every exit, a panic included. This repository's
`docs/rules/tui.md` carries these forward as promises every crate makes.
