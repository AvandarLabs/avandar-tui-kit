# Architecture spec

The design the workspace is built from: what each crate holds, how the crates
depend on each other, the conventions every public API follows, and the
policies for platforms, color, text width, releases and tests.

[`../architecture.md`](../architecture.md) stays the short description of the
workspace as it is. This spec is the plan it grows toward. Everything in
[`../decisions.md`](../decisions.md) is settled and is not reopened here. Where
this spec decides something new, the decision is also recorded there.

The per-crate specs (the first is [`ratatui-mouse.md`](ratatui-mouse.md)) give
each crate's public API. This spec gives what they share.

## Goals

- An app compiles only what it turns on. The heavy dependencies (a terminal
  emulator, a pseudo-terminal library, a markdown parser, a fuzzy matcher, a
  tree widget, an async runtime, a command-line parser) each sit behind one
  crate or one feature.
- Every part is something an app calls. No crate owns the event loop, the
  app's state, or the terminal for longer than a guard the app holds.
- Every part behaves the same in iTerm2, in xterm.js, in tmux inside either,
  and on macOS, Linux and Windows, or degrades in a way its docs describe.
- A part is testable by calling functions. Decisions are pure, time is an
  argument, and what a part writes to the terminal is bytes a test can read.

## Crate map

Eight building blocks and one umbrella. A crate is created when its own spec
is written, not ahead of it.

| Crate | Purpose | Features | Depends on (workspace) |
| --- | --- | --- | --- |
| `ratatui-mouse` | Mouse modes, hit areas, clicks, selection, wheel routing, pass-through into panes | `vt100` | none |
| `avandar-tui-term` | Borrowing the terminal and giving it back, keyboard flags, background color, clipboard | `mouse` | `ratatui-mouse` behind `mouse` |
| `avandar-tui-pty` | Embedded terminal panes, key encoding, child supervision | `mouse` | `avandar-tui-widgets`; `ratatui-mouse` behind `mouse` |
| `avandar-tui-agent` | Coding-agent frontends: launch, resume, typing and submitting prompts, message queues | `pty`, `resume-evidence`, `uuid` | `avandar-tui-pty` behind `pty` |
| `avandar-tui-widgets` | Style, layout, commands and keys, pickers, trees, input and forms | `fuzzy`, `tree`, `tree-walk`, `clap`, `jiff`, `mouse` | `ratatui-mouse` behind `mouse` |
| `avandar-tui-document` | Document opening, viewing and (in M9.b) opt-in editing: markdown, scrollable documents, logs, open targets, nano/vim editing | `markdown` (plus editor features if M9.a selects an upstream crate) | `avandar-tui-widgets` |
| `avandar-tui-charts` | Text charts and tables | none | `avandar-tui-widgets` |
| `avandar-tui-tasks` | Background work a UI needs | `process`, `tokio` | none |
| `avandar-tui-kit` | The umbrella. Owns no code. | see [The umbrella crate](#the-umbrella-crate) | all eight, each optional |

No crate has default features. Every feature is additive, and every feature
combination compiles.

### Dependency graph

```
avandar-tui-kit ........ every crate below, each one optional

avandar-tui-agent -----> avandar-tui-pty         only with feature `pty`
avandar-tui-pty -------> avandar-tui-widgets
avandar-tui-pty -------> ratatui-mouse           only with feature `mouse`
avandar-tui-document --> avandar-tui-widgets
avandar-tui-charts ----> avandar-tui-widgets
avandar-tui-widgets ---> ratatui-mouse           only with feature `mouse`
avandar-tui-term ------> ratatui-mouse           only with feature `mouse`

ratatui-mouse .......... no workspace dependency
avandar-tui-tasks ...... no workspace dependency
```

The rules behind the graph:

- **No cycles, and nothing depends on the umbrella.**
- **`ratatui-mouse` depends on no `avandar-tui-*` crate.** Its name promises a
  crate any ratatui app can take alone.
- **`avandar-tui-tasks` depends on no other workspace crate and not on
  ratatui.** Background work is not drawing.
- **`avandar-tui-widgets` is the only crate three others need
  unconditionally**, and they need only its `style` module (see
  [Where shared things live](#where-shared-things-live)). With no feature on
  it pulls nothing beyond ratatui, crossterm and `unicode-segmentation`.
- **A dependency between two parts that only some apps combine is a
  feature**, never a hard dependency: mouse support in the terminal guard,
  the pane as an agent transport, hit areas for widgets.

### External dependencies

Versions are declared once in `[workspace.dependencies]` and are the ones
current in November 2026. Each is added to the root manifest, with its
one-line reason, in the change that creates the first crate that uses it.

| Dependency | Used by | Behind feature | Why |
| --- | --- | --- | --- |
| `ratatui` 0.30 | every crate except tasks | always | The rendering library |
| `crossterm` 0.29 | every crate except tasks and charts | always | Backend and event types |
| `unicode-segmentation` 1 | widgets, mouse | always | Grapheme clusters, for the one width function and for columns in app-held text. Already in ratatui's own tree. |
| `vt100` 0.16 | mouse, pty | `vt100` in mouse; always in pty | The terminal emulator. Held at the version `tui-term` builds against. |
| `tui-term` 0.3 | pty | always | Draws a `vt100` screen into a ratatui buffer |
| `portable-pty` 0.9 | pty | always | Pseudo-terminals on Unix, ConPTY on Windows |
| `nix` (Unix only) | pty | always | Signals a child's process group without `unsafe` |
| `nucleo-matcher` 0.3 | widgets | `fuzzy` | The one fuzzy matcher |
| `tui-tree-widget` 0.24 | widgets | `tree` | The tree widget |
| `walkdir` 2 | widgets | `tree-walk` | Directory walking for the file tree |
| `clap` 4 | widgets | `clap` | Menus generated from a command-line definition |
| `jiff` 1 | widgets | `jiff` | Calendar types for the time dial and time field |
| `pulldown-cmark` 0.13 | document | `markdown` | The markdown parser |
| `open` | document | always | Opens a file or link with the system handler on all three platforms |
| `process-wrap` 10 | tasks | `process` | A child in a process group on Unix and a job object on Windows, behind one API |
| `tokio` 1 | tasks | `tokio` | Async variants of the helpers |
| `serde_json` 1 | agent | `resume-evidence` | Reading agents' session files |
| `uuid` 1 | agent | `uuid` | New session ids |

Not dependencies, on purpose:

- **No error-derive crate.** Error enums implement `Display` and
  `std::error::Error` by hand.
- **No base64 crate.** The clipboard's OSC 52 route needs twenty lines of
  encoder.
- **No scrollbar code.** A part that needs a scrollbar uses ratatui's own
  `Scrollbar` widget. An app that wants a draggable one takes `tui-scrollbar`
  itself.
- **No mouse-encoding crate.** The pass-through encoder is a small pure
  function in `ratatui-mouse`.

### `ratatui-mouse`

Mouse support for ratatui apps, usable alone. Its spec is
[`ratatui-mouse.md`](ratatui-mouse.md).

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `modes` | The enable and teardown sequences, a guard, the Windows path | dif: mouse modes |
| `hit` | `HitAreas<T>`, rebuilt every frame; clamping a pointer into an area | dif: pane hit-testing, pointer clamping. brain: wheel routing by pane |
| `clicks` | Forgiving multi-click counting | dif: multi-click tracker |
| `controller` | The gesture state machine: the press owns the gesture, selection stages, auto-scroll deadline, redraw budget | dif: drag-to-copy controller |
| `selection` | The selection in content coordinates, units, stages | dif: selection model |
| `highlight` | A selection as at most three rectangles, and painting them | dif: highlight |
| `surface` | The `SelectableSurface` trait and the surface for app-held lines | dif: text extraction from rendered lines |
| `words` | Word and line finders over plain lines, measured in cells | dif: word and line finders |
| `passthrough` | What a pane's program asked for, event encoding, wheel to arrow keys | new: no source app forwards the mouse |
| `select_mode` | Keyboard selection, so every mouse action has a key | new |
| `vt100` (feature) | The surface over a `vt100` screen across scrollback, and reading the program's mouse request | dif: text extraction from a terminal screen, word and line on a real screen |

### `avandar-tui-term`

Borrows the terminal and always gives it back.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `guard` | An ordered list of steps (raw mode, screen, paste, focus, keyboard flags, mouse), each recorded so a partial start rolls back. Restore is idempotent, keeps going on errors and runs on drop. Profiles for a full-screen UI and for an inline prompt. | brain: terminal guard with recorded capabilities, ordered startup and shutdown lifecycle. dif: terminal setup and teardown. huckleberry-cli: terminal session owner on stderr, scoped raw-mode guard. quickstarter: terminal guard |
| `panic` | A panic hook installed once per process that restores before the message prints | quickstarter: panic hook. huckleberry-cli: its once-only hook |
| `screen` | Where the UI draws: stdout, stderr, or the controlling terminal | brain: drawing on the controlling terminal. huckleberry-cli: drawing on stderr |
| `keyboard` | Kitty keyboard flags pushed without probing and popped on restore, bracketed paste, focus events | brain: kitty flags in the guard. huckleberry-cli: bracketed paste |
| `input` | Waiting for a key or a whole paste with a timeout; draining queued input while remembering a force-quit chord | huckleberry-cli: terminal session owner |
| `colors` | The background color query and its reply parser (see [Theme](#theme)) | new |
| `clipboard` | Routes: a local tool per platform, OSC 52 plain and wrapped for tmux. The app picks or accepts the default order. | dif: clipboard |
| `environment` | Pure reads of what the process is in: tmux, SSH, the terminal's name, `NO_COLOR`, whether a stream is a terminal | dif, brain: scattered checks |

The capability probe (see [`../terminals.md`](../terminals.md)) is this
crate's first example program.

### `avandar-tui-pty`

A child program in a pane.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `pane` | Spawning into a pseudo-terminal, reader, writer and waiter threads, a writer that paces writes by each write's settle delay, resize that is a no-op when unchanged, scrollback, the app's own lines written into the screen, respawn into the same screen, kill on drop | brain: embedded terminal pane. dif: embedded terminal pane, activity log written into a pane, restart a child in place |
| `env` | A cleared environment plus an allowlist, with a default list per platform | brain: environment allowlist |
| `keys` | A crossterm key as the bytes the child expects: application cursor mode, modified arrows, function keys, Insert | brain, dif: key events to bytes, with the gaps filled |
| `paste` | A paste forwarded as a bracketed paste when the child asked for it | new: fixes the unbracketed paste in dif and brain |
| `screen` | A trait over the emulator's screen, so `vt100` stays out of other crates' signatures | brain: the lesson from `vt100` in a public trait |
| `render` | Drawing the screen, placing the real cursor when focused and alive, the exited-child placeholder | dif: embedded terminal renderer, exited-child placeholder. brain: agent pane renderer (the terminal part) |
| `supervise` | Pure decisions: a child counts as started after it stays alive for a set time, relaunch in place or close on exit, a refused resume is remembered | brain: crash-on-arrival detector, pane supervision |

The `mouse` feature adds a `ratatui-mouse` surface for a pane and sends the
bytes the mouse crate asks to forward.

### `avandar-tui-agent`

Frontends for coding agents. Its core has no heavy dependency: a pane is one
transport among others.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `input` | An input sequence: writes with settle delays, text as one bracketed paste with control characters stripped, the submit key as its own write | brain: input sequences. dif: typing a prompt into an agent (replaced by brain's) |
| `adapter` | The adapter trait: intents (type, submit, follow up, new session) to input sequences, how completion is detected, whether a session can resume | brain: agent adapter trait |
| `adapters` | Reference adapters for Claude Code, Codex, OpenCode and pi: input keys and launch flags | brain: per-agent input keys, launch command builders. dif: agent kinds |
| `launch` | The launch spec (command, directory, environment, redacted debug output), the check that a configured command is a plain call of a binary | brain: launch spec, command parser. dif: launch command line |
| `quote` | Shell quoting for POSIX shells and for Windows | brain, dif: quoting |
| `resume` | The resume-or-fresh plan. Evidence from each agent's own session storage behind `resume-evidence`. | dif: session resume. brain: resume evidence per agent |
| `transport` | The transport trait: spawn, send, snapshot, liveness, shutdown, optional screen, resize and scroll | brain: agent transport trait |
| `controller` | One adapter plus one transport: launch with rollback, semantic input, key forwarding, idempotent shutdown | brain: agent controller, agent pane key forwarding |
| `registry` | An open table of adapters | brain: agent registry |
| `queue` | Items turned into prompts exactly once after a baseline, three delivery modes, the held set persisted, a reply watcher | dif: message queue into the agent |

The `pty` feature implements the transport over `avandar-tui-pty`. A toolkit
default never skips an agent's permission prompts.

### `avandar-tui-widgets`

Everything that needs nothing heavier than ratatui, in six module families.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `style::theme` | Roles, the four palettes, picking a palette from a background color | huckleberry-cli: semantic theme. quickstarter: semantic theme. brain: dark palette (replaced by roles) |
| `style::oklab` | Color conversion and a perceptual lighten | brain: Oklab color math |
| `style::width` | The one cell-width function and the portable-text rewrite (see [Text width](#text-width)) | fixes "character counts instead of terminal cells" in all three apps |
| `style::wrap` | Wrapping styled spans at whitespace, soft-wrap for input, measuring wrapped height | dif: style-preserving word wrap. brain: soft-wrap for input, logical-to-visual row measuring |
| `style::fit` | Truncation with an ellipsis, elision that keeps a filename's head and extension, fitting columns to a width | brain: label elision, truncation. huckleberry-cli: column-width allocator |
| `layout::rect` | A centered rect clamped to its area, an inset rect | dif, brain, quickstarter: centered rect. huckleberry-cli: centered modal helpers |
| `layout::split` | Split panes with focus, a swappable side | dif: two-pane shell with focus. brain: split with a swappable side |
| `layout::tabs` | Tabs with ids that never repeat, numbered strip, wrapping cycle, digit jumps, drawn into any rect, opening without taking focus | brain: tabs with stable ids, numbered tab strip, view cycle, sub-view strip, background tabs rule. dif: view switcher and tab strip. huckleberry-cli: tabbed dashboard |
| `layout::modal` | One optional modal with open, replace and close, and one routing function | brain: single-owner modal host. huckleberry-cli: modal precedence |
| `layout::frame` | Header, footer, focus-bordered panels, a content-sized drawer, a responsive two-column layout | huckleberry-cli: shell frame, content-sized drawer. brain: frame composition (the helpers) |
| `layout::status` | A status line with tiers (flash, background, warning, hint), a notice with expiry, a persistent error banner | brain: three-tier status line, persistent error banner. dif: statusline with a transient notice |
| `command::catalog` | The command vocabulary as the parent set of every command, and what a command needs first | quickstarter: command vocabulary and catalog. brain: command catalog, target gating |
| `command::palette` | `CommandPalette<A>`: numbered rows, word filter, wrapping navigation, its renderer | brain: generic command palette, palette renderer. quickstarter: palette with word-atom filtering. dif: palette state and renderer (replaced) |
| `command::shortcuts` | One shortcuts table, the parity checks an app runs in its tests, the width-aware footer, the grouped help modal | brain, quickstarter: shortcuts table with parity guards. brain: shortcut footer, grouped help modal. dif: shortcuts table, help modal, statusline chips |
| `command::motion` | Keys to semantic motions with a configurable table, the vim grammar with counts | huckleberry-cli: one-handed key map. dif: vim motion grammar. brain: vim count prefix and list motions |
| `command::keys` | Press-only filtering, chord classifiers, the Option-glyph decoders, the layer a key belongs to | brain: event classification, key classifiers, Option-glyph chord decoders, layered key dispatch. dif: layered key capture. huckleberry-cli: input router. quickstarter: key classifier |
| `pick::menu` | A menu stack with drill-down, remembered levels and a breadcrumb, its renderer. Submenus from clap behind `clap`. | huckleberry-cli: menu stack, menu renderer, menu tree bound to clap |
| `pick::select` | A numbered scrolling selection prompt | huckleberry-cli: selection prompt. quickstarter: arrow-key prompts (replaced) |
| `pick::listing` | A searchable, grouped, column-aligned list with choose and show modes | huckleberry-cli: list view |
| `pick::list` | A variable-height list with a selection band | brain: variable-height list |
| `pick::fuzzy` | The fuzzy picker: word runs, separator-insensitive matching with highlight remap, grouped results, match line and chrome (`fuzzy`) | brain: fuzzy picker, separator-insensitive matching, grouped results with headers, match line and picker chrome |
| `pick::filter` | Inline filter mode | brain: inline filter mode |
| `pick::quick` | Numbered quick-pick with disabled but visible rows, a toggle checklist | brain: quick-pick, picker with disabled rows, toggle checklist |
| `tree` | Tree from a flat path list, expand to a target, re-rooting, state, keys, file kinds, the panel renderer (`tree`). The directory walker (`tree-walk`). | brain: the seven tree elements |
| `input::line` | The single-line editor shared by fields, palettes and pickers | brain: query editing. huckleberry-cli: text input |
| `input::text` | Text field with horizontal scroll, inline error, validator, masked mode, paste-aware | brain: single-line input, prefilled input. huckleberry-cli: text input, secret input |
| `input::compose` | Multi-line compose | brain: multi-line compose |
| `input::confirm` | One confirm dialog: yes and no with a safe default, an optional third choice, a bound target line | brain: both confirms. dif: confirm modal, with its arrow keys fixed |
| `input::choice` | A scrollable body above a list of choices | brain: choice dialog with scrollable details |
| `input::dial` | The dial picker, the amount-with-units dial, the time dial (`jiff`) | huckleberry-cli: the three dials |
| `input::form` | The multi-step form engine with retained drafts, control adapters, a form paired with its executor, the result hold | huckleberry-cli: form engine, form control adapters, prepared command, result hold and retry. brain: captive wizard (as a form) |
| `input::question` | The question model, its plain-text fallback, the failure that names the flag | huckleberry-cli, quickstarter: question model. huckleberry-cli: typed cancel and home outcomes (as outcome enums) |

The `mouse` feature adds, to each widget state that records areas, a method
that registers them in a `ratatui_mouse::HitAreas<T>`.

### `avandar-tui-document`

Opening, viewing and (in M9.b) editing documents in different forms.

M9.a delivers read-only parity for the source apps, preceded by an ecosystem
survey of existing ratatui document-viewer and text-editor crates so the
buffer, cursor and settings (`is_editable` off by default, plus editor-style
options) are shaped for editing from the start and can wrap a solid upstream
crate where one fits. M9.b adds opt-in editing with caller-selectable
nano-style (modeless chords) and vim-style (`Normal` and `Insert` modes with
common cursor motions) editing.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `markdown` (feature) | Markdown to ratatui text at a width, tables, the inline style stack | dif: markdown to ratatui text, box-drawn table, style stack |
| `document` | A styled document view whose cursor is the source of truth, motions, a mode with no cursor, and settings shaped for M9.b's opt-in nano and vim editing | dif: document view with a cursor. huckleberry-cli: pager |
| `log` | A scrollable log, a follow mode fed from outside the draw, a bounded tail read | brain: log viewer, live log-tail modal, bounded log-tail export. dif: activity log (when not a pane) |
| `target` | The token under a cursor as a URL or a path with a line, links found in text | dif: open what the cursor is on (parsing). brain: links in text |
| `open` | The `Opener` trait and the built-in ones: the system handler, reveal in the file manager, a new tmux window | dif: opener. brain: OS actions |

### `avandar-tui-charts`

Charts and tables drawn with characters every first-class terminal measures
the same.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `bar` | A horizontal bar at eighth-cell resolution, bars over time | huckleberry-cli: eighth-block bars, horizontal bars over time |
| `sparkline` | A sparkline always scaled from zero | huckleberry-cli: sparkline from zero |
| `stripes` | Timeline stripes, one row per period | huckleberry-cli: timeline stripes |
| `columns` | Vertical bars with an average line | huckleberry-cli: daily vertical bars |
| `plot` | A line plot of several series with a staggered axis, shape-preserving interpolation | huckleberry-cli: line plot |
| `table` | A browsable data table with grouped headers, a box-drawn table with a stacked fallback | huckleberry-cli: browsable data table, receipt table |
| `format` | Padding, a dash for a missing value | huckleberry-cli: format helpers |

### `avandar-tui-tasks`

Background work, on standard threads and channels.

| Module | Holds | Inventory elements that land here |
| --- | --- | --- |
| `task` | At most one cancellable task: start refuses while one is pending, collect never waits, drop aborts | huckleberry-cli: single cancellable background task |
| `last_good` | The last good reading, a refreshing flag and a failure note | huckleberry-cli: keep-last-good data holder |
| `watch` | A value computed on an interval on a thread, the latest one published, joined on drop. A take-once slot. | dif: latest-value watcher thread, the take-once slot of its control endpoint |
| `debounce` | A settle debouncer as a pure state machine with the time passed in | brain: debouncer. dif: debounced change alert |
| `jobs` | Jobs with ids that settle apart from the dialog that started them, serial lanes, pending changes overlaid on the last known data | brain: job coordinator with worker lanes and optimistic updates |
| `process` (feature) | A child in its own group or job: streamed lines, capped output, a timeout, killed on drop | brain: cancel-on-drop subprocess worker, command probe |
| `input_queue` | Input queued until the next question is ready, the last frame kept meanwhile | huckleberry-cli: input queue with last-frame retention |

The `tokio` feature adds async forms of `task`, `watch` and `process`.

### What is left out

Use count is not the test: an element one app has is still a candidate. These
are left out for the reasons given, and each reason is the condition for
taking the element back in.

| Element | App | Why it is out |
| --- | --- | --- |
| The event loop, tick stages, shell loop skeleton, standalone dashboard loop | all | The toolkit never owns the loop. Each crate's example program shows one. |
| State with a single update function, frame composition | quickstarter, brain | An app shape, not a part. Documented as the pattern the examples follow. |
| Per-pane scroll dispatch | dif | A three-arm match in the app. The toolkit gives every scrollable part the same method names instead (see [Public API conventions](#public-api-conventions)). |
| ANSI-to-ratatui bridge, prompt host bridge, panel for a running command | huckleberry-cli | Ruled out by the decision that prompts are native widgets. A widget draws into any rect, so the bridge has nothing to carry. |
| Command running inside a panel | huckleberry-cli | Built on that bridge. An app composes `tasks::task` with `input::form`. |
| Second fuzzy matcher | brain | One matcher was decided. |
| Small markdown renderer | brain | dif's renderer covers it. |
| Context-aware row wording | brain | Wording is the app's. The palette takes each row's label when it opens. |
| Hub form, prerequisite gates | huckleberry-cli | Patterns over the form engine, documented with it. |
| Option editors generated from clap, edit-a-copy settings menu, Ctrl-C interruption | huckleberry-cli | Dormant in the app: no production path reaches them, so their design is unproven. |
| Single-prompt tabs that close on a callback | brain | Needs a channel the agent calls back on, which is the app's to provide. A later agent release may add it. |
| Lifecycle hooks and progress observation | brain | Writes into each agent's settings and breaks when an agent changes them. |
| Session store, default-agent setting | brain | App configuration. |
| Local control endpoint | dif | An HTTP server and its dependency. Only its take-once slot is generic, and `tasks::watch` has it. |
| Detached process with a progress marker | brain | Detaching differs per platform and the marker format is the app's. Documented as a pattern beside `tasks::process`. |
| Live-session metadata file, one-instance lock, port picking, slugs, atomic writes, pluralizer, relative dates, shell runner seam | several | Small utilities that are not UI. |
| Semantic theme for command-line output, stripping ANSI codes | brain | Not ratatui. The roles are shared, the plain-text output path is not. |
| Version modal, dashboard panel recipes | huckleberry-cli | App features built from parts the toolkit has. Recipes go in the charts docs. |
| Moving a file to the trash | brain | File management. An app takes the `trash` crate. |
| Scrollbars | brain | ratatui's `Scrollbar` is used directly. |

## Where shared things live

| Shared thing | Lives in | How a crate that cannot depend on it gets by |
| --- | --- | --- |
| Theme roles and palettes | `avandar_tui_widgets::style::theme` | `ratatui-mouse` and `avandar-tui-term` take a ratatui `Style` or return a ratatui `Color`. They never name a role. |
| The cell-width function and the portable-text rewrite | `avandar_tui_widgets::style::width` | `ratatui-mouse` calls ratatui's own `CellWidth` per grapheme cluster, which is what the function is defined as. |
| Wrapping, truncation, column fitting | `avandar_tui_widgets::style` | Only widgets, document, charts and pty need them. |
| Hit areas | `ratatui_mouse::HitAreas<T>` | A widget records plain ratatui `Rect`s in its state. The app, or the widget's `mouse` feature, registers them. |
| Outcome enums | A convention, not a type | Each part defines its own enum. See below. |
| Errors | A convention, not a type | Each crate defines its own enum. See below. |
| Events, rectangles, colors, styles | crossterm's and ratatui's own types | The vocabulary between crates is the two libraries every crate already has. |

`style` is written as if it were its own crate: it imports nothing from its
sibling modules, and a test in the widgets crate fails if it does. That keeps
the ninth-crate question (see [Open questions](#open-questions)) a matter of
moving a directory.

The pieces fit through the app, not through each other. The terminal crate
returns a background color, the style module turns a color into a theme, and
the app passes one to the other. The names here are illustrations until those
crates have their specs:

```rust
let background = avandar_tui_term::colors::query_background(timeout)?;
let theme = Theme::from_background(background.color, is_color_disabled);
```

## Public API conventions

Every crate follows these. A per-crate spec may add to them and never departs
from them.

### State the app owns

- A part with memory is a state type the app creates and keeps:
  `PaletteState`, `MouseController<T>`, `FormState`. Fields are private and
  change through methods named for what happened, never through setters.
- A part with no memory is a function.
- Drawing reads state. A widget is `impl Widget for &Part` or a
  `StatefulWidget` whose state holds only what drawing itself produces (the
  areas it drew, a scroll offset it clamped). Drawing never decides what
  happens next.
- No part takes `&mut App`, a callback into the app, or a channel to the app.
  No part has global state.

### Pure decision functions

- What a key means, where a click landed, which rows a filter keeps, the
  bytes to write: each is a function from values to values, public when an app
  could want to test or replace it.
- Functions that read a key take `&KeyEvent` and act on `Press` and `Repeat`.
  A `Release` returns the part's ignored outcome, because Windows reports
  both.
- Reading the environment, the clock or the terminal happens in a thin shell
  around the pure function. The pure function takes what was read as an
  argument.

### Outcome enums

A part returns what happened and the app decides what it means.

- Every handler returns an enum named `{Part}Outcome`, marked `#[must_use]`
  and `#[non_exhaustive]`. A part that has more to say with each outcome
  (whether a redraw is due, for one) returns a small `{Part}Report` struct
  that holds the outcome.
- Two variants mean the same in every crate: `Ignored` (not for this part:
  the app handles the event as it would have) and `Consumed` (handled, nothing
  for the app to do).
- Other variants are named for what happened, in the past tense, and carry
  the values the app needs to act: `Accepted(value)`, `Cancelled`,
  `Clicked { target }`. A variant that asks the app to do something the part
  must not do itself is named for the thing to do: `Copy { text }`,
  `Forward { bytes }`.
- A part never performs an effect it can report instead: it does not write to
  the clipboard, send bytes to a child, move focus, or open a link.

### Typed errors

- One enum per crate, named `{Crate}Error` (`MouseError`, `TermError`,
  `PtyError`), marked `#[non_exhaustive]`. A crate with unrelated failure
  families has one per module.
- `Display` reads as one sentence that says what the crate was doing.
  `source()` returns the cause. Variants carry the cause as a field, never as
  a formatted string.
- Every fallible public function has an `# Errors` section, and every function
  that can panic a `# Panics` section. Library code does not panic on input:
  no `unwrap` or `expect` outside tests.
- Being unable to do something on one platform is a variant
  (`Unsupported { what }`), never a silent success.

### Time

- Time is an argument. A function that depends on the present takes
  `now: Instant`. A part that needs a calendar date takes it as a value.
- A part that needs time to pass has two methods and no thread:
  `next_deadline(&self) -> Option<Instant>`, which the app folds into its poll
  timeout, and `tick(&mut self, now: Instant, ...)`, which the app calls on
  every pass of its loop and which returns what the part's handlers return.
- Durations are settings with documented defaults.
- The exceptions are the parts whose job is a thread (a pane's reader and
  writer, a watcher). Their constructors say so in their docs.

### Settings

A part's tunable values are one struct named `{Part}Settings` with public
fields, `Default`, and `#[non_exhaustive]`. Defaults are the values the source
apps use today unless a spec says otherwise.

### Scrolling

Every part that scrolls has the same methods, so an app's scroll dispatch is a
match with identical arms: `scroll_by(rows: i32) -> i32` (negative is toward
the start, returns the rows actually moved), `scroll_to_start()`,
`scroll_to_end()`, and `page_rows() -> u16`.

### Naming

The naming rules in `AGENTS.md`, as they read in Rust:

| Rule | In Rust |
| --- | --- |
| Builder for a type | `Type::new(...)`, or `Type::from_{seed}(...)` when built from seed data |
| Builder for a string or primitive | `build_{thing}(...)`, or `build_{thing}_from_{seed}(...)` |
| Conversion | `to_{thing}` as a method, `{from}_to_{thing}` as a free function |
| Booleans | `is_`, `has_` or `should_`, for fields, variables and functions |
| No abbreviations, no vague names | `column`, `position`, `selected_row`, `num_rows`; never `col`, `pos`, `next` |
| State, settings, outcome, report, error | `{Part}State`, `{Part}Settings`, `{Part}Outcome`, `{Part}Report`, `{Crate}Error` |
| Module names | The singular noun for what the module holds: `palette`, `selection`, `clipboard` |

### Features

- A feature is named for the dependency it brings in (`vt100`, `clap`,
  `tokio`) or the capability it adds (`fuzzy`, `markdown`, `mouse`).
- A feature only adds items. It never changes the behavior of an item that
  exists without it.
- Items behind a feature are marked so the API docs show the feature's name.

## The umbrella crate

`avandar-tui-kit` re-exports each building block as a module, behind a feature
named for the block. Nothing is on by default.

| Feature | Re-exports | As |
| --- | --- | --- |
| `mouse` | `ratatui-mouse` | `avandar_tui_kit::mouse` |
| `term` | `avandar-tui-term` | `avandar_tui_kit::term` |
| `pty` | `avandar-tui-pty` | `avandar_tui_kit::pty` |
| `agent` | `avandar-tui-agent` | `avandar_tui_kit::agent` |
| `widgets` | `avandar-tui-widgets` | `avandar_tui_kit::widgets` |
| `document` | `avandar-tui-document` | `avandar_tui_kit::document` |
| `charts` | `avandar-tui-charts` | `avandar_tui_kit::charts` |
| `tasks` | `avandar-tui-tasks` | `avandar_tui_kit::tasks` |

Features that pass through to a block's heavy option, each turning its block
on as well:

| Feature | Turns on |
| --- | --- |
| `fuzzy`, `tree`, `tree-walk`, `clap`, `jiff` | `widgets`, and that feature of the widgets crate |
| `markdown` | `document`, and `markdown` of the document crate |
| `process`, `tokio` | `tasks`, and that feature of the tasks crate |
| `resume-evidence`, `uuid` | `agent`, and that feature of the agent crate |
| `full` | Everything. For the API docs and for tests. |

Glue between two blocks turns on by itself when both are on, through cargo's
`crate?/feature` form, so no app has to know the glue exists:

| When these are both on | The umbrella also turns on |
| --- | --- |
| `mouse` and `term` | `avandar-tui-term/mouse`: the guard's mouse step |
| `mouse` and `pty` | `avandar-tui-pty/mouse`, which brings `ratatui-mouse/vt100` |
| `mouse` and `widgets` | `avandar-tui-widgets/mouse`: hit-area registration |
| `agent` and `pty` | `avandar-tui-agent/pty`: the pane transport |

So `mouse` alone never compiles a terminal emulator, and `mouse` with `pty`
gets selection inside panes without naming `vt100`.

## Platform policy

No crate assumes Unix. Each concern below has one module that holds the
platform split, chosen with `#[cfg]`, and the rest of its crate has no platform
branches. Platform calls go through a maintained crate, never hand-written
bindings, because `unsafe` is forbidden.

| Concern | macOS and Linux | Windows | Module | Through |
| --- | --- | --- | --- | --- |
| Raw mode, the alternate screen, reading events | crossterm | crossterm | `term::guard` | crossterm |
| Drawing on the controlling terminal instead of stdout | Open `/dev/tty` | Open the console output, `CONOUT$` | `term::screen` | `std::fs` |
| Mouse reporting | Write the mode sequence | Turn on mouse input in the console's input mode. No sequence is written. | `ratatui_mouse::modes` | crossterm |
| Kitty keyboard flags | Push on start, pop on restore | Not available through the console API: the step reports `Unsupported` and the guard skips it | `term::keyboard` | crossterm |
| Bracketed paste | Turn on mode 2004 | crossterm reads console records and reports no paste event, so mode 2004 is left off. `term::input` joins a burst of key events that arrive in one read into one paste. | `term::keyboard`, `term::input` | crossterm |
| Focus events | Turn on mode 1004 | Always reported by the console | `term::keyboard` | crossterm |
| Key releases | Not reported | Reported: every handler acts on presses only | every handler | convention |
| A child in a pseudo-terminal | A Unix pseudo-terminal | ConPTY | `pty::pane` | portable-pty |
| Launching through the user's shell | `$SHELL` with `-c`, and `-l` or `-i` when the app asks, so aliases resolve | No login shell: the program is found through `PATH` and `PATHEXT`, and a `.cmd` or `.bat` runs through `cmd.exe /D /S /C`. Asking for a login shell is not an error and changes nothing. | `agent::launch`, `pty::pane` | std, portable-pty |
| Shell quoting | POSIX single-quote rules | The rules `cmd.exe` and the Windows argument parser apply | `agent::quote` | pure functions |
| Stopping a pane's child and its descendants | Signal the child's process group, then kill it after a grace period | Close the pseudo-console, which ends every process attached to it | `pty::pane` | nix, portable-pty |
| Stopping any other child and its descendants | A process group | A job object | `tasks::process` | process-wrap |
| Interrupting a pane's child | Write the interrupt byte to the pane | The same | `pty::pane` | none |
| The default environment allowlist | `HOME`, `PATH`, `SHELL`, `TERM`, `LANG`, `LC_*`, `USER`, `TMPDIR` | `SystemRoot`, `PATH`, `PATHEXT`, `COMSPEC`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP`, `USERNAME` | `pty::env` | data |
| The local clipboard tool | macOS: `pbcopy`. Linux: `wl-copy` when Wayland is in use, then `xclip`, then `xsel`. Under WSL: `clip.exe`. | `clip.exe`, fed UTF-16 so text outside ASCII survives | `term::clipboard` | `std::process` |
| The clipboard over OSC 52 | Plain, and wrapped for tmux when inside tmux | Plain | `term::clipboard` | bytes |
| Opening a file or link | `open`, `xdg-open` | The system handler | `document::open` | the `open` crate |
| Revealing a file | macOS: `open -R`. Linux: open the parent directory. | `explorer /select,` | `document::open` | `std::process` |
| An editor in a new window | A new tmux window when inside tmux. Otherwise the app's own `Opener`. | The app's own `Opener` | `document::open` | `std::process` |
| The home directory | `std::env::home_dir` | The same | `style::fit`, `tree` | std |

Rules that follow from the table:

- **A test that runs on one platform has a sibling on the others** wherever
  the behavior exists there, as `docs/rules/rust.md` requires.
- **A gap is an outcome or an error the app can see.** Where Windows cannot
  do what Unix does (kitty flags, aliases), the part says so through
  `Unsupported` or its docs. It never pretends.
- **Windows rows are designs until the Windows runners prove them.** Windows
  runs only on CI (see [`../testing.md`](../testing.md)), so each row is
  measured there when its crate is built, and
  [`../terminals.md`](../terminals.md) records what was found.

## Theme

Color is semantic: a part asks for a role and the theme maps it to a style.
The theme picks its palette from the terminal's background color, which every
first-class terminal reports, and has a palette for when nothing answers.

### Roles

Eleven roles, enough for every toolkit part. An app with roles of its own
keeps its own map beside the theme.

| Role | For |
| --- | --- |
| `Heading` | A section title |
| `Accent` | A key, a command name, a label, the focused border |
| `Value` | Text that carries the answer |
| `Selected` | The selected row, selected text |
| `Muted` | Secondary text, an unfocused border |
| `Faint` | Text meant to recede further than muted |
| `Success`, `Warning`, `Error`, `Info` | Status |
| `Prompt` | A question waiting on an answer |

Color never carries meaning alone: wherever a role distinguishes two things,
a marker, a modifier or a word distinguishes them too.

### Palettes

Four palettes. Three use the terminal's own sixteen colors by index, so the
user's color scheme is inherited. The starting values:

| Role | Dark | Light | Unknown | No color |
| --- | --- | --- | --- | --- |
| `Heading` | bright magenta, bold | magenta, bold | magenta, bold | bold |
| `Accent` | bright cyan | blue | cyan | plain |
| `Value` | bright white | default foreground, bold | default foreground, bold | bold |
| `Selected` | see below | see below | reversed | reversed |
| `Muted` | bright black | bright black | dim | dim |
| `Faint` | bright black, dim | bright black, dim | dim | dim |
| `Success` | bright green | green | green | plain |
| `Warning` | bright yellow | yellow | yellow | bold |
| `Error` | bright red | red | red | bold |
| `Info` | bright blue | cyan | cyan | plain |
| `Prompt` | bright cyan, bold | blue, bold | cyan, bold | bold |

- **Dark** is the palette the source apps use: the bright half of the
  sixteen colors.
- **Light** uses the normal half, which is the half drawn to be read on a
  light background, and never white.
- **Unknown** is for a terminal that did not answer. It uses only what reads
  on both a dark and a light background: the default foreground, bold, dim
  and reversed, and red, green, yellow, magenta and cyan from the normal
  half. It never uses black, white, their bright forms, or blue.
- **No color** uses modifiers only.

`Selected`, when the background color is known, is the background moved by
0.125 toward the foreground in Oklab lightness (lighter on a dark background,
darker on a light one), with the text's own color kept. When it is not known,
`Selected` is reversed video.

Whether each pairing reads well is a judgment on the release checklist. The
structure (roles, four palettes, the rule for each) is the decision.

### Detection

The order, first match wins:

1. **The app's own setting**: dark or light, when the app has one and the
   user set it. The toolkit reads no theme variable of its own.
2. **`NO_COLOR` present and not empty**: the no-color palette. Nothing is
   asked of the terminal.
3. **The terminal's answer** to the background color query: dark or light by
   luminance.
4. **No answer in time**: the unknown palette.

The query, in `avandar_tui_term::colors`:

- It is sent once, after raw mode is on and before the first frame: OSC 11
  with a question mark.
- The reply is read **through crossterm's events**, not from the terminal's
  bytes. crossterm delivers an OSC reply as key events (Alt and `]`, the
  characters of the reply, then the terminator), so a pure parser over
  `Event` recognizes it. This is the same code on all three platforms, and
  on Windows there is no other way to read it.
- The wait ends when the reply is complete or after 200 ms, whichever is
  first. Events that are not part of the reply are handed back to the app in
  the order they arrived.
- **A late reply never reaches the app as key presses.** The parser stays
  armed for two seconds after the query. A reply that arrives in that window
  is swallowed and returned as a color, and the app may switch palettes on
  its next frame, which costs nothing because drawing is a pure function of
  state.
- The reply's three channels may each have one to four hex digits. The
  background is dark when its relative luminance is below 0.179, the point
  where white text and black text contrast with it equally.

Why a wait and a fallback instead of an assumption: a fresh iTerm2 has a
light background, so "assume dark" is wrong on a default install. tmux hands
the query to the outer terminal, and inside iTerm2 the answer did not come
before tmux's own replies, so the order of replies cannot be relied on. On
Windows the reply has to cross the pseudo-console, which is not measured yet.
In every one of those cases the unknown palette is readable, which a wrong
guess is not.

The app may ask again whenever it likes (on a command, on regaining focus).
The toolkit does not watch for the terminal's colors changing.

## Text width

How many cells a string takes is decided by the terminal, and iTerm2, xterm.js
and tmux disagree on emoji sequences (see
[`../terminals.md`](../terminals.md)). The policy has four parts.

**1. One function.** `avandar_tui_widgets::style::width::cell_width(text)` is
the only way the workspace measures text. It splits the text into extended
grapheme clusters and sums ratatui's own `CellWidth` for each cluster. That is
exactly what ratatui does when it places text in a buffer, so a width the
toolkit measured and a width ratatui drew can never differ.

- Nothing in the workspace counts characters or bytes for layout.
- Nothing calls `unicode-width` directly, or ratatui's `Span::width` and
  `Line::width`. Those measure a whole string at once and can differ from
  what the buffer does: for an Arabic lam followed by an alef they return 1,
  while the buffer, and all three terminals, use 2.
- A `clippy.toml` entry disallows those methods. It is added with the first
  crate that measures text.

**2. The toolkit draws only what all three agree on.** Borders, bars, markers
and sparklines come from the rows of the table in `terminals.md` where the
three terminals match. No toolkit part emits an emoji sequence of its own, and
a test in each drawing crate checks its glyph constants against the allowed
set.

**3. Text from the app is made portable before it is drawn.**
`text_to_portable(text)` rewrites the three kinds of cluster the terminals
disagree on into a form they agree on. Each rule comes from a measurement:

| Cluster | iTerm2 | xterm.js | tmux | Raw `cell_width` | Portable form | Width after |
| --- | --- | --- | --- | --- | --- | --- |
| An emoji with a variation selector (a red heart) | 1 | 1 | 2 | 2 | The selector is removed | 1 in all three |
| An emoji with a skin tone | 2 | 4 | 2 | 2 | The skin tone is removed | 2 in all three |
| A ZWJ sequence that starts with a one-cell character (the rainbow flag) | 1 | 1 | 2 | 2 | Cut to its first character | 1, to be confirmed: the flag alone is not in the first measurements |
| A ZWJ sequence that starts with a two-cell character (a family) | 2 | 2 | 2 | 2 | Unchanged | 2 |
| A cluster of zero cells (a zero-width space) | 1 | 1 | 0 | 0 | Removed | 0 |

- Every toolkit function that turns app text into spans or cells calls it
  first: wrapping, truncation, column fitting, markdown, list rows, the
  display of an input field.
- It rewrites **what is drawn, never what is stored.** An input field's value,
  a document's source and text copied from a pane keep their original
  characters.
- It returns its input unchanged, without allocating, when there is nothing
  to rewrite.
- A kind of cluster the probe has not measured is not rewritten and not
  claimed. Each rule has a sample in the capability probe, so a terminal
  release that changes an answer shows up as a failed check.

**4. A pane's grid belongs to its emulator.** Inside an embedded terminal the
emulator has already decided which cell each character is in. The pane
renderer draws each cell where the emulator put it and applies the same
rewrite to each cell's contents, so a cell the emulator made one wide is drawn
one wide.

What the toolkit does not do: keep a width table per terminal. tmux in front
changes the answer, an SSH hop hides which terminal is there, and such tables
go stale with every terminal release.

## Versioning, release and minimum Rust version

### Versions

- **One version for the whole workspace**, in `[workspace.package]`. Every
  crate, `ratatui-mouse` included, carries it.
- A release changes it. Ordinary changes do not.
- Before 1.0 a breaking change to any crate's public API bumps the minor
  version of all of them, and anything else bumps the patch version.
- Moving to a new ratatui or crossterm minor version is a breaking change,
  because their types are in the toolkit's public APIs.
- Workspace crates name each other in `[workspace.dependencies]` with both a
  `path` and a `version`, so the same manifests work from a git tag now and
  from crates.io later.

### Release

Until the first building block has a real user, a release is a git tag.

1. Every test layer the changes since the last tag can affect has passed,
   the local ones included (Orca, the iTerm2 virtual machine).
2. The short checklist of judgments in `testing.md` is done by a person.
3. One commit bumps the version and updates `CHANGELOG.md`, which is kept by
   hand from the Conventional Commits since the last tag.
4. The commit is tagged `vMAJOR.MINOR.PATCH` on `main`. One tag covers every
   crate.

Tagging and pushing are done by a person or with their explicit say-so. When
publishing to crates.io starts, `release-plz` takes over steps 3 and 4 and
publishes in dependency order.

### Published READMEs and crate docs

- **Honesty about wrappers.** When a crate or module is a light wrapper or glue
  layer over upstream crates, its README and rustdoc say so upfront, explain
  the specific functionality added on top, and recommend using the base crate
  directly when a caller does not need that extra layer. We do not try to sell
  visitors on our crates over the base crates they wrap.
- **No personal workflow leaks.** Published READMEs, `Cargo.toml` fields, and
  rustdoc never mention personal or internal source apps (`brain`,
  `huckleberry-cli`, and similar private workflows). Those references stay in
  internal planning docs under `docs/`.

### Minimum Rust version

- `rust-version` is 1.88, the floor ratatui 0.30 sets, declared once.
- CI builds and tests at exactly that version, with every feature on, so an
  optional dependency cannot raise the floor unnoticed.
- A dependency whose own floor is higher than the workspace's is not added
  without a decision.
- Raising the floor is a breaking change and waits for a release that is
  breaking anyway.

## How an app adopts a crate

An app depends on a tag, never on a branch.

```toml
# Most apps: the umbrella, with the parts they use.
avandar-tui-kit = { git = "https://github.com/AvandarLabs/avandar-tui-kit", tag = "v0.1.0", features = ["mouse", "term"] }

# Or one building block alone.
ratatui-mouse = { git = "https://github.com/AvandarLabs/avandar-tui-kit", tag = "v0.1.0", features = ["vt100"] }
```

The steps, one part at a time:

1. **Match the base.** The app is on ratatui 0.30, crossterm 0.29 and Rust
   1.88 or newer. An app on older versions upgrades first, as its own work.
   An app that shares a type with a feature's dependency (`vt100`, `jiff`,
   `clap`) is on the same major version of it.
2. **Take every toolkit crate from the same tag.** Two tags would compile two
   copies and their types would not pass between each other. The umbrella
   makes this automatic.
3. **Replace one module with one part.** The app keeps its state and its
   event loop. It creates the part's state type, calls the part from its
   handlers, and matches on the outcome where its own code used to act.
4. **Delete the local copy and the tests the crate now owns.** Tests of the
   app's own behavior stay.
5. **Run the app's tests and its own terminal checks.**

Nothing forces an app to adopt more than one part, and no part requires
another unless the crate map says so.

| App | First parts | Before it can start |
| --- | --- | --- |
| dif (`AvandarLabs/avandar`) | `ratatui-mouse` with `vt100`, then the terminal guard and clipboard | Nothing: it is on the base versions already |
| brain (`jpsyx/brain`) | The terminal guard (which fixes its mouse-mode bug), then the pane | Its move to ratatui 0.30 and crossterm 0.29 |
| huckleberry-cli (`jpsyx/huckleberry-cli`) | Style, charts, tasks with `tokio` | It uses `jiff` 0.2, so the `jiff` feature waits for its upgrade |
| quickstarter (`AvandarLabs/quickstarter`) | Its `rust-tui` template takes the palette, shortcuts and terminal guard | A later decision on timing |

## Testing

[`../testing.md`](../testing.md) defines the seven layers and the rules. This
section says what each crate provides so the layers have something to drive.

### What every crate provides

- **Layer 1.** Unit tests beside every pure function, written red first.
  Drawing is tested against ratatui's `TestBackend` buffer. Anything a crate
  writes to the terminal is asserted as bytes.
- **Layer 2.** A clean cross-target lint: every `#[cfg]` branch type-checks
  for the Windows and Linux targets from a Mac.
- **Feature checks.** CI checks each crate with no features, with each
  feature alone, and with all of them. One command per row, no extra tool.

### What a crate that touches the terminal provides

It ships an **example program the harnesses drive**, under its `examples/`
directory, that follows the rules in `testing.md`:

- It takes `--log <path>` and appends one JSON object per line: every event
  it received, every outcome a part returned, every deadline it scheduled,
  and every sequence it wrote to the terminal.
- It writes a `ready` line, and creates `<path>.ready`, only after its
  terminal modes are written, so a harness can wait on a condition.
- It exits cleanly on one documented key, and has a documented key that
  panics, so teardown is tested on both paths.
- It reads no clock for its decisions except to pass `now` to the parts.

| Crate | Layers | Example programs | What the harnesses assert |
| --- | --- | --- | --- |
| `ratatui-mouse` | 1 to 7 | `mouse_lab` | See the test plan in its spec |
| `avandar-tui-term` | 1 to 7 | `capability_probe`, `term_lab` | The probe's answers match `terminals.md`. Every mode the guard set is reset after a clean exit, an error and a panic. Each clipboard route emits its bytes. The background reply is parsed and a late one is swallowed. |
| `avandar-tui-pty` | 1 to 4, 6, 7 | `pane_lab`, with `echo_input` as its child | Keys, pastes and forwarded mouse events typed at the outer terminal arrive in the child as the expected bytes. A resize reaches the child. Killing the pane leaves no process behind. |
| `avandar-tui-agent` | 1, 2, and 3 and 7 through `pane_lab` | A stub agent that records what it was sent and when | A prompt arrives as one paste, and the submit key as a later, separate write. No real agent runs in CI. |
| `avandar-tui-widgets` | 1, 2, and 3 to 7 for key decoding | `widget_lab` | Chords, Option glyphs and Alt keys decode to the same motions in every configuration |
| `avandar-tui-document` | 1, 2 (and keyboard harness if M9.b adds key paths beyond M10) | none in M9.a | The opener's command line is asserted per platform as a pure function, with nothing launched; M9.b asserts read-only gating plus nano and vim editing states. |
| `avandar-tui-charts` | 1, 2 | none | Buffers, and the glyph check from [Text width](#text-width) |
| `avandar-tui-tasks` | 1, 2, 7 | none | Real threads and real children in `cargo test` on all three platforms. No child outlives its handle. |
| `avandar-tui-kit` | 1 | none | Every feature compiles alone and all together |

A crate is not done until its row passes in every configuration. The test rig
(the xterm.js harness, the tmux wrapper, the iTerm2 driver) lives outside the
published crates and is the first milestone, with the capability probe.

## Open questions

Each has a recommendation. None blocks the mouse crate.

1. **Do the shared primitives need a ninth crate?** `avandar-tui-document`,
   `avandar-tui-charts` and `avandar-tui-pty` depend on `avandar-tui-widgets`
   only for its `style` module. *Recommendation: stay at eight for now.*
   With no feature on, widgets is plain Rust over ratatui, so the cost is
   compile time and not a dependency. `style` is kept free of its siblings,
   so it can become `avandar-tui-style`, re-exported from widgets at the same
   path, without breaking an app. Split it the first time one of these is
   true: widgets with no features gains a dependency, a crate outside the
   three needs a role or a width, or a charts-only app's clean build is
   measurably slowed by widgets.
2. **Should the portable-text rewrite have an off switch?** It changes how a
   few emoji look (a heart loses its color presentation, a thumbs-up its skin
   tone). *Recommendation: no switch in the first release.* The alternative
   is misaligned columns and stale cells in two of the three terminals. Add
   a switch when an app shows a need, as a setting on the style functions.
3. **Does `ratatui-mouse` keep the workspace's version once it is on
   crates.io?** *Recommendation: yes until it has users outside the toolkit,
   then let `release-plz` version it alone.* One version keeps the tag phase
   simple.
4. **How are agents launched on Windows when they are installed as `.cmd`
   shims?** The table above runs them through `cmd.exe`. *Recommendation:
   keep that, and prove it on the Windows runners with a stub `.cmd` before
   the agent crate's spec is final.*
5. **Does the background color reply cross the Windows pseudo-console, and
   does a paste arrive as one burst?** Neither is measured.
   *Recommendation: build both as designed and measure on the runners with
   the probe. If no reply arrives, the unknown palette is the documented
   Windows behavior. If a paste is not one burst, pasting into an input on
   Windows is documented as arriving key by key.*
6. **Is the answer inside tmux in iTerm2 late or missing?** `terminals.md`
   marks it to verify. *Recommendation: no design change either way.* The
   wait, the armed parser and the unknown palette cover both.
