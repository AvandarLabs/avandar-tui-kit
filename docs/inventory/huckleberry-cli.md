# Inventory: huckleberry-cli

`huckleberry-cli` is a menu-driven shell with dashboards, forms and pickers,
for a baby-tracking service. It has no embedded terminal, no agent and no
mouse support.

- **Repository:** `jpsyx/huckleberry-cli`. Paths below are repo-relative.
- **Versions:** edition 2024, Rust 1.85, ratatui 0.30, crossterm 0.29,
  tokio 1 (multi-thread runtime), clap 4, jiff 0.2.
- **Event loop:** an async function on tokio that reads crossterm input
  synchronously with a timeout. Each pass collects finished background reads,
  collects the running command's output and questions, draws, then waits for
  one input. It ticks every second when idle and every 30 ms while a command
  runs.
- **Screen:** the shell draws on **stderr** with the alternate screen, so
  stdout stays free for data. Standalone full-screen views use ratatui's own
  init on stdout.
- **State shape:** a strict pure/impure split. A key map turns keys into
  motions, state turns motions into intents, `draw/` renders, and one module
  touches the terminal. Every component has its own small action enum (stay,
  accept, back, home, cancel, quit in different spellings).
- **Two rendering worlds:** prompts, listings, charts and tables render to
  lines of text with ANSI codes; the shell turns those back into styled spans.
  That gives one implementation of every question across an inline prompt, a
  full-screen view and a panel inside the shell, at the cost of a lossy round
  trip (see the ANSI bridge below). Whether the toolkit's prompts are line
  based or native ratatui widgets is a design decision for the specs.
- **Keys:** one-handed. Arrows, `hjkl` and `wasd` mean the same thing. Digits
  highlight and only Enter acts. Esc goes home, Ctrl-C cancels one level,
  Ctrl-Q always quits. Key hints are always on screen; there is no help
  overlay.

Line counts are production lines, without inline tests, and approximate.
Elements marked **dormant** compile and are tested but no production path
reaches them. Categories are the ones defined in [`README.md`](README.md).

## Terminal

### Terminal session owner on stderr `term`
Raw mode, alternate screen, bracketed paste and a hidden cursor on stderr,
restored in a deliberate order on drop, plus a panic hook installed once per
process. Waits for a key or a whole paste with a timeout, and can drain queued
input while remembering whether a force-quit chord was in it.
- **Where:** `src/tui/shell.rs` (about 190)
- **Generic:** all of it; the writer and the panic policy become parameters.
- **Effort:** low. **Tests:** none, by design.
- **Lessons:** its own once-only hook, because `ratatui::init` wraps the hook
  again on every call. Rebuild the terminal rather than clear it after
  resuming, because clearing queries the cursor and blocks on stdin.

### Scoped raw-mode guard for inline prompts `term`
Raw mode and bracketed paste without the alternate screen, restored on drop or
by an explicit restore that reports errors.
- **Where:** `src/prompt/terminal.rs` (about 35)
- **Effort:** low. **Tests:** none.

## Commands and keys

### One-handed key map `cmd`
Turns a key into a semantic motion: next, previous, first, last, highlight N,
open, back, home, cancel, refresh, quit, focus left or right. Ignores key
releases (Windows reports both).
- **Where:** `src/tui/keys.rs` (about 115)
- **Generic:** all of it; the table should be configurable.
- **Effort:** low. **Tests:** yes (34).
- **Lessons:** `q` quits only when the shell has the keys; inside a text field
  it is a letter.

### Focus-aware input router with modal precedence `layout`
Force-quit first, then an open modal, then focus chords, then the focused
panel, then the running command, then the dashboard, then the menu.
- **Where:** `src/tui/input.rs` (about 130)
- **Generic:** the precedence. **App-specific:** the hard-wired panels; a
  toolkit would use a focus and modal stack.
- **Effort:** medium. **Tests:** yes.

## Menus, lists and pickers

### Menu stack with drill-down and breadcrumb `pick`
The left and right menu navigation: each level keeps its own cursor and
scroll; opening a submenu pushes a level and back pops it; a popped level is
remembered so reopening restores its cursor. A breadcrumb shows the path.
- **Where:** `src/tui/state.rs` (about 375)
- **Generic:** the stack, remembered levels, breadcrumb, cursor logic.
  **App-specific:** app fields mixed into the same struct.
- **Effort:** medium. **Tests:** yes.
- **Lessons:** refresh never moves the cursor.

### Menu tree bound to clap `pick`
Declares the menu tree, generates submenus from clap subcommands, adds Back
rows, and walks the tree so a test proves every command is reachable from the
menu.
- **Where:** `src/interactive/catalog.rs` (about 245)
- **Generic:** clap-derived submenus and the coverage walk.
- **Deps:** clap. **Effort:** low to medium. **Tests:** yes.

### Menu renderer `pick`
Right-aligned row numbers of equal width, a bar on the cursor row, a chevron
at the right edge for rows that open a submenu, labels clipped to the cell
width.
- **Where:** `src/tui/draw/menu.rs` (about 70)
- **Effort:** low. **Tests:** partial.

### Numbered scrolling selection prompt `pick`
Pure cursor and scroll state. Movement keys move, digits highlight, Enter
submits, Esc or Left cancel. Rows wrap with a hanging indent and the scroll
window keeps a multi-line cursor row whole. Runs inline on stderr or inside
the shell.
- **Where:** `src/prompt/select/` (about 310)
- **Effort:** low. **Tests:** yes.
- **Lessons:** "a number that submits is a number that logs a feed when
  somebody meant to scroll".

### Searchable, grouped, column-aligned list view `pick`
Rows with a key, a group, cells, a tone, an optional note, a selectable flag
with a reason, and detail pairs. `/` opens a live filter where every word must
appear somewhere in the row; group headings stay while any of their rows
match. Choose mode returns a key, show mode shows the details. Renders as
plain aligned text when not on a terminal, full screen, or inside the shell.
- **Where:** `src/listing/` (about 1,200)
- **Generic:** all but a "today" heading rule. **Effort:** low to medium.
  **Tests:** yes.

### Column-width allocator `pick`
Natural widths from the widest cell; when they do not fit, the narrowest
columns keep their width first and wide ones share the rest.
- **Where:** `src/listing/layout.rs`
- **Effort:** low. **Tests:** yes.
- **Gap:** counts characters, not terminal cells.

### Pluralizer for status counts (marginal) `pick`
"1 entry", not "1 entries".
- **Where:** `src/listing/model.rs` (about 45). **Effort:** low.

## Input, forms and dialogs

### Question model: flag, config, prompt, or a failure naming the flag `input`
A question names its subject, label, help, the flag that answers it, choices,
default, and whether it can be skipped. A pure interpreter parses typed
answers by number, name or default. With no terminal, it fails with a message
naming the flag.
- **Where:** `src/prompt/mod.rs`, `src/prompt/question.rs`,
  `src/prompt/select/question.rs` (about 400)
- **Effort:** low. **Tests:** yes. Same pattern as quickstarter's prompt
  module.

### Text input `input`
Single-line entry: Enter submits, Esc and Ctrl-C cancel, Ctrl-U clears. A paste
lands whole with control characters stripped, so a pasted newline cannot
submit. Long input shows the tail that fits.
- **Where:** `src/prompt/text.rs` (about 200)
- **Effort:** low. **Tests:** partial.
- **Gap:** no cursor movement within the line.

### Secret input `input`
Masked entry where Ctrl-C cancels instead of submitting partial input and a
Ctrl+letter is never typed as the letter.
- **Where:** `src/prompt/secret.rs` (about 45)
- **Effort:** low. **Tests:** yes.

### Typed cancel and home outcomes `input`
Two error markers that distinguish back, escape-to-home and real failures
through any wrapping context.
- **Where:** `src/prompt/mod.rs` (about 30). **Effort:** low.

### Dial picker `input`
Columns of values ("wheels") that cycle or stop at their ends, with an active
column, a leap size on Shift, a landmark such as `[now]` reachable with `n`,
an optional Tab switch to another representation, and animated turns that stop
as soon as another key arrives. Shows two values either side of the chosen
one, fading with distance, with a rule under the active column so color is not
the only signal.
- **Where:** `src/prompt/dial/` core (about 780)
- **Generic:** fully; "adding a third dial is a list of labels and two
  numbers". **Effort:** low to medium. **Tests:** yes, extensive.

### Time dial `input`
A relative mode ("20 min ago") and a clock mode (hour, minute, am/pm), with
Tab converting between them without losing the value, and a lower bound that
shrinks the columns.
- **Where:** `src/prompt/dial/time.rs` (about 325)
- **Deps:** jiff. **Effort:** medium (calendar types come with it).
  **Tests:** yes.

### Amount-with-units dial `input`
An amount column and a units column; turning the units converts the amount and
lands on the nearest notch. Tab changes the step.
- **Where:** `src/prompt/dial/volume.rs` (about 185)
- **Generic:** the pattern, with a units table as a parameter.
- **Effort:** low to medium. **Tests:** yes.

### Multi-step form engine with retained drafts `input`
A form model lists its views in order, creates each view, validates each
answer and builds a typed output. The form keeps every view's state, so going
back never destroys earlier or later answers, and a view reopened after its
constraints change keeps unfinished input. Validation errors show under the
view. Back from the first view suspends the form.
- **Where:** `src/prompt/flow/` engine (about 700)
- **Generic:** the engine. **App-specific:** unit types inside some values.
- **Effort:** medium. **Tests:** yes, extensive.
- **Lessons:** the clock is always passed in.

### Form control adapters `input`
A time field that follows the clock until first edited, an amount that keeps a
canonical value so switching units never drifts, a number with a unit system,
and a searchable list used as a form view that refuses to silently select a
neighbor when its row disappears.
- **Where:** `src/prompt/flow/{time,volume,measurement,listing}.rs` (about 570)
- **Effort:** medium. **Tests:** yes.

### Prepared command with suspend and resume `input`
Pairs a form with an executor that consumes it, so navigation can never
replay a submission. Tracks collecting, suspended, ready, executing and
result; reopening the same command resumes its draft.
- **Where:** `src/commands/prepared/` (about 150), `src/tui/forms.rs` (about
  140)
- **Effort:** medium. **Tests:** yes.

### Hub form: pick fields to change (pattern) `input`
An edit form whose route returns to a "fields to change" menu after each field
and submits only what changed.
- **Where:** `src/commands/edit/retained.rs` and `retained/` (about 650)
- **Effort:** medium, or document as a pattern.

### Prerequisite gates (pattern) `input`
Missing prerequisites (sign-in, a choice, a setting) become forms that run
before the command, then the original command continues.
- **Where:** `src/commands/prepared/gates.rs` (about 200)
- **Effort:** document as a pattern.

### Option editors generated from clap (dormant) `input`
Collects argument values by clap id and resolves them through clap's own
parser without a shell. Classifies each argument into a control (choice,
boolean, text, secret, repeated) and clears conflicting arguments.
- **Where:** `src/interactive/draft.rs` (about 140),
  `src/interactive/options/` (about 540)
- **Deps:** clap. **Effort:** low to medium. **Tests:** yes.

### Edit-a-copy settings menu (dormant) `input`
Edits a copy of the session options, then applies or discards.
- **Where:** `src/interactive/session_menu.rs` (about 140). **Tests:** none.

### Result hold and retry loop `input`
A one-row "Continue" hold for static output, and a loop that offers Retry
only after a failed read, never after a write.
- **Where:** `src/interactive/mod.rs`, `src/interactive/operation.rs` (about
  150)
- **Effort:** low. **Tests:** partial.

## Navigation and layout

### Shell frame: header, footer, focus-bordered panels, drawer `layout`
A fixed three-part frame. The header shows the title and breadcrumb; the
footer shows a status (reading, failure, transient note) and the context's key
hints. The focused panel gets a thick accented border and nothing else
changes. A content-height drawer on top gives way first on a short terminal;
the bottom panels keep a floor of 10 rows.
- **Where:** `src/tui/draw/mod.rs` (about 300)
- **Effort:** medium. **Tests:** yes, extensive (80x24 fit, resize reflow).
- **Lessons:** the frame never moves, because people read by position. Hints
  are patched with string replacement per host; a toolkit should take hints
  as data.

### Content-sized drawer with a responsive column layout `layout`
Measures the drawer's height at the moment it is drawn. Two columns with a
divider at 96 cells and wider, stacked below that. Label and value rows wrap
with a hanging indent.
- **Where:** `src/render/now/layout.rs` (about 200), `src/tui/draw/now.rs`
- **Effort:** low to medium. **Tests:** yes.
- **Lessons:** "a widget is only as tall as it has something to say".

### Centered modal helpers `layout`
A centered rect clamped to the terminal, and an inset rect whose margins
shrink on small terminals. Drawn over a cleared area.
- **Where:** `src/tui/draw/version.rs`, `src/tui/draw/summary.rs`
- **Effort:** low. **Tests:** yes.

### Tabbed dashboard drawable in any area `layout`
Tabs that wrap, digit jumps, a tab bar, a body and a status bar, all drawn
into a given rect. That one parameter let the standalone dashboard become a
view inside the shell.
- **Where:** `src/dashboard/state.rs` (about 290), `src/dashboard/draw/mod.rs`
  (about 145)
- **Effort:** low to medium. **Tests:** yes.

### Panel inside the shell for a running command `layout`
Draws a running command's output tail and current question in a focus-aware
panel and reports the panel's size back so prompts lay out to it.
- **Where:** `src/tui/draw/flow.rs` (about 40)
- **Effort:** low. **Tests:** yes.

### Help browser and pager `view`
A menu of every command; choosing one shows its clap long help in a scrollable
pager.
- **Where:** `src/interactive/mod.rs`, `src/prompt/host.rs` (about 70)
- **Effort:** low. **Tests:** partial.

## Charts and tables

### Format helpers: eighth-block bars, sparkline from zero `chart`
A horizontal bar at 1/8-cell resolution where zero draws nothing, a sparkline
always scaled from zero, padding, and a dash for missing values.
- **Where:** `src/render/format.rs` (about 270, part domain-specific)
- **Effort:** low. **Tests:** yes.
- **Lessons:** "a chart scaled from its own minimum turns 11, 12, 13 into a
  crisis and a recovery".

### Timeline stripes, one row per period `chart`
Each row is a period split into cells. Blocks and ticks are placed by their
fraction of the row's own length, so 23- and 25-hour days fill the row. Layers
have a precedence, a ruler places hour labels where they fall, and today's row
is brightest.
- **Where:** `src/render/stripes.rs` (about 170),
  `src/dashboard/draw/panels.rs` (about 60 of it)
- **Effort:** low to medium. **Tests:** yes.

### Horizontal bars over time `chart`
One line per day: a label, a value and a bar scaled from zero to the peak,
with today marked as still in progress.
- **Where:** `src/render/trends.rs` (about 160)
- **Effort:** low. **Tests:** yes.

### Daily vertical bars with an average line `chart`
Vertical bars at 1/8 resolution with a y-axis gutter and a dotted average
line that keeps the bar's background where it crosses one.
- **Where:** `src/render/summary/chart.rs` (about 165)
- **Effort:** low. **Tests:** yes.

### Line plot with a staggered time axis `chart`
Rasterizes several series into a character grid, telling them apart by pattern
as well as color, with markers, a "nice" y maximum, and time labels pushed
into extra rows when crowded. Includes a shape-preserving interpolation.
- **Where:** `src/render/summary/comparison/` (about 560),
  `src/domain/cumulative/interpolation.rs` (about 90)
- **Effort:** medium. **Tests:** yes.

### Browsable data table with grouped headers `chart`
A fixed first column plus a window of columns under grouped headers, with a
cell cursor, a window that moves only when the cursor crosses an edge, hints
that shorten to fit, and a chart sharing the height below.
- **Where:** `src/render/summary/` (about 1,200 with the app's catalog)
- **Generic:** navigation, grouped headers, the height split.
  **App-specific:** a static catalog of metrics.
- **Effort:** medium to high. **Tests:** yes.

### Receipt table with a stacked fallback `chart`
A box-drawn table measured in terminal cells that falls back to stacked
"Header: value" records when it does not fit.
- **Where:** `src/render/output.rs` (about 150 generic of 340)
- **Effort:** low. **Tests:** yes.

### Dashboard panel recipes (marginal) `chart`
Big-fact rows, a table above a sparkline, a scrollable numbered log.
- **Where:** `src/dashboard/draw/panels.rs` (about 355). **Effort:** low.

## Style

### Semantic theme with NO_COLOR and separate stdout and stderr gating `style`
Roles mapped to bright ANSI palette indexes, so the user's terminal palette is
inherited. Color is on only when the channel is a terminal and `NO_COLOR` is
unset, judged separately for stdout and stderr. Generic roles (heading, accent,
value, selected, muted, faint, success, warning, error, info, prompt) plus app
roles.
- **Where:** `src/theme.rs` (about 315)
- **Effort:** low. **Tests:** yes (every role readable on a dark background).
- **Lessons:** color never carries meaning alone.

### ANSI-to-ratatui bridge `style`
Parses themed ANSI lines back into roles and ratatui spans.
- **Where:** `src/tui/draw/mod.rs`, `src/dashboard/draw/mod.rs`
- **Effort:** low. **Tests:** yes.
- **Lesson:** lossy when two roles share a code. Prefer rendering to spans
  natively.

## Background work

### Prompt host bridge `task`
While a host is installed, prompts send their frames to it over a channel and
get input back, so a prompt written once renders inline or inside a panel. The
host reports its panel size; a vanished host reads as a cancellation.
- **Where:** `src/prompt/host.rs` (about 250), `src/render/mod.rs` (about 50)
- **Effort:** medium: the global slot and blocking channels need a redesign
  for a library. **Tests:** yes.

### Input queue with last-frame retention `task`
Queues input that arrives before the next question is ready and hands it out
one item per question, so fast typing never loses characters. Keeps the last
frame on screen while the next is pending.
- **Where:** `src/tui/job.rs` (about 90 of it)
- **Effort:** low. **Tests:** yes.
- **Lessons:** fixed a bug where typing "6:30am" arrived as "63a".

### Command running inside a panel `task`
Runs a command on a task, collects its output and questions without blocking,
and reports how it ended (home, stay, cancelled, failed). Writes are never
retried automatically.
- **Where:** `src/tui/job.rs` (about 330), `src/tui/job/work.rs` (about 70)
- **Effort:** high (tied to forms and the host). **Tests:** partial.

### Keep-last-good data holder `task`
Holds the last successful reading, a refreshing flag and a failure note; a
failure never discards the data.
- **Where:** `src/tui/facts.rs` (about 100), `src/tui/data.rs` (about 100)
- **Effort:** low. **Tests:** partial.
- **Lessons:** "a screen that blanks itself when the wifi drops is worse than
  one that admits it is showing something from a minute ago".

### Single cancellable background task `task`
At most one task; start refuses while one is pending, collect never waits,
cancel and drop abort it, a panic reads as unavailable.
- **Where:** `src/tui/version_task.rs` (about 50)
- **Effort:** low. **Tests:** yes.

### Version modal with an update check `task`
Opens at once with the installed version, then fills in the result of a
background release check (up to date, update available, offline). Closing
aborts the request so an old result never lands in a reopened modal.
- **Where:** `src/tui/version.rs`, `src/tui/draw/version.rs`,
  `src/version/` (about 370)
- **Effort:** low. **Tests:** yes.

### Shell loop skeleton `task`
The loop shape: a background read, a cancellable side task, an optional
running command and an optional suspended draft, with the tick rate chosen by
what is running.
- **Where:** `src/tui/mod.rs` (about 205)
- **Effort:** medium, as a reference rather than a part.

### Ctrl-C interruption of awaited work (dormant) `task`
Races an operation against Ctrl-C and warns that a submitted request may have
completed.
- **Where:** `src/interactive/interrupt.rs` (about 15)

### Standalone dashboard loop `task`
Refuses to start without a terminal and suggests the plain-text command;
re-reads on a key or a timer.
- **Where:** `src/commands/dash.rs` (about 125)
- **Lesson:** the re-read blocks keys; the shell's background read is the
  version to keep.

## Out of scope

- A natural-language time parser (`src/domain/clock.rs`), typed TOML
  settings, credentials storage: not UI.

## Drift found while reading

- `docs/decisions.md` still describes the shell suspending itself for commands
  and a picker module, both since replaced.
- Several widths count characters while others count terminal cells.
