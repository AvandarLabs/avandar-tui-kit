# Inventory: brain

`brain` is a persistent shell: tasks, fuzzy search, a file tree and logs on
the left, coding-agent sessions in tabs on the right. It is the largest
source, with about 53,000 lines under `src/tui/` alone.

- **Repository:** `jpsyx/brain`. Paths below are repo-relative.
- **Versions:** edition 2024, Rust 1.85, **ratatui 0.29** (with the
  `unstable-rendered-line-info` feature for wrapped-height measuring),
  **crossterm 0.28**, **vt100 0.15**, **tui-term 0.2**, tui-tree-widget 0.23.1
  (pinned), portable-pty 0.9, nucleo-matcher 0.3, fuzzy-matcher 0.3.
- **Event loop:** one synchronous UI thread: tick, draw, poll for 50 ms, read,
  update. It redraws every pass. Background work runs on OS threads and
  detached child processes; there is no async runtime. Paste, resize and focus
  events are ignored, and bracketed paste is never enabled.
- **Screen:** draws to `/dev/tty`, so stdout stays clean.
- **State shape:** an `App` composed of focused owners with private fields and
  semantic transitions. Commands are one closed enum run by one executor, so
  every hotkey and its palette row call the same code (tested). Exactly one
  `Option<Overlay>` owns modals, and input routing and drawing match the same
  enum, so stacked modals cannot be represented. Keyboard focus (which side)
  and the main view (which content) are separate axes.
- **Architecture guards:** tests parse the source with `syn` to enforce module
  boundaries, for example that no agent adapter is named outside `src/agent/`.
- **Theme:** a dark palette defined twice plus scattered RGB literals.

## Upgrade to ratatui 0.30

brain must move to ratatui 0.30 and crossterm 0.29 before it can use toolkit
crates. These move together: ratatui, crossterm, tui-tree-widget 0.24 (which
follows ratatui's 0.30 crate split), and a tui-term release built on ratatui
0.30, which pins vt100 too. vt100 appears in a public trait
(`AgentTransport::terminal_screen`), so the toolkit should hide the emulator
behind its own trait. The `unstable-rendered-line-info` feature must stay.

Line counts are production lines, without inline tests, and approximate.
Categories are the ones defined in [`README.md`](README.md).

## Terminal

### Terminal guard with recorded capabilities `term`
Acquires raw mode, `/dev/tty`, the alternate screen, mouse capture and the
kitty keyboard flags in a fixed order, recording each so a partial start rolls
back. Restore is idempotent, keeps going on errors, and retries on drop. Has a
trait seam for tests.
- **Where:** `src/tui/runtime/terminal.rs` (about 200)
- **Effort:** low. **Tests:** yes, pins the order.
- **Lessons:** the release order is deliberately not a mirror of the
  acquisition order. The kitty flags are pushed without probing for support,
  because the probe can race teardown and leak bytes into the parent shell.
  **Known bug:** it calls `EnableMouseCapture` and then turns modes 1002 and
  1003 off, which in iTerm2 turns the wheel off entirely (dif's notes explain
  why). No panic hook: restore relies on unwinding.

### Ordered startup and shutdown lifecycle `term`
A fixed acquisition order that errors when violated, and a fixed, idempotent
shutdown order reused by drop. Early returns during startup still unwind what
was taken.
- **Where:** `src/tui/runtime/{mod,shutdown,builder}.rs` (about 550)
- **Effort:** medium, as a generic lifecycle. **Tests:** yes.

### Recurring tick stages (pattern) `task`
Background work per frame runs as named stages in a test-pinned order, so the
loop stays "tick, draw, poll, update".
- **Where:** `src/tui/runtime/tick.rs` (about 95)

### Event loop and event classification `cmd`
Reduces crossterm events to ignore, mouse, or key (press and repeat only), and
gives Ctrl+Q and the palette's Quit row one shared exit that refuses while
background saves are pending.
- **Where:** `src/tui/event_loop/run.rs` (about 290)
- **Effort:** low. **Tests:** yes.

### Layered key dispatch `cmd`
Quit, then the modal, then Esc for the error banner, then app chords, then
main-panel-only accelerators (kept off the agent pane so readline chords reach
it), then the focused panel.
- **Where:** `src/tui/event_loop/run.rs`
- **Effort:** medium (needs a binding table). **Tests:** yes, 12 files.

### Key classifiers and terminal portability notes `cmd`
Small pure functions for each chord, with comments recording what terminals
actually send.
- **Where:** `src/tui/keymap.rs` (about 250), `src/main_view.rs` (about 150)
- **Effort:** low. **Tests:** yes.
- **Lessons:** without the kitty protocol Ctrl+H arrives as Backspace and
  Ctrl+M as Enter. Shift+Enter is identical to Enter unless every key is
  reported, which breaks text input. Alt+Enter is Escape then CR everywhere.
  Alt+letter was chosen over a leader key because both alternatives collide
  with editing in an agent's prompt.

### macOS Option-glyph chord decoders `cmd`
Reads Alt+digit, Alt+brackets and Alt+U/D both as real Alt chords and as the
characters macOS's Option key types on a US layout (`¡`, `™`, `£` and so on),
so they work with either terminal setting. An unbound glyph falls through as
text, so `£50` still types.
- **Where:** `src/tui/keymap.rs` (about 100)
- **Effort:** low. **Tests:** yes.

### One-instance lock (marginal) `term`
A PID file that is reclaimed when its process is gone.
- **Where:** `src/tui/singleton.rs` (about 55). **Effort:** low.
- **Gaps:** reclaim race; PID reuse.

## Mouse

### Wheel routing by pane, half-page scroll `mouse`
The draw pass records the agent pane's rect; the wheel scrolls whatever pane
is under the pointer, focused or not. Modals swallow the wheel. Alt+U/D
scroll half a page as the dependable keyboard path.
- **Where:** `src/tui/handlers/input.rs` (about 45), `src/tui/keymap.rs`
- **Effort:** low. **Tests:** partial.

## Embedded terminal

### Embedded terminal pane `pty`
Spawns a command in a pseudo-terminal with a cleared environment plus an
allowlist, feeding a shared vt100 parser through reader, writer and waiter
threads. The writer sleeps each write's settle delay first, so pacing never
blocks the UI. Resizing is a no-op when unchanged, so it is safe every frame.
Shutdown kills and waits up to 2 s. 10,000 rows of scrollback; typing snaps to
the live tail.
- **Where:** `src/pty_pane.rs` (about 360)
- **Effort:** low. **Tests:** yes, 12 on a real pseudo-terminal.
- **Lessons:** a non-login shell keeps filtered secrets from coming back
  through shell profiles. Reader and writer errors are swallowed today.

### Key events to bytes `pty`
Characters, control codes, Alt as an Escape prefix, Enter, Tab, Backtab,
Backspace, Escape, arrows, Home, End, PgUp, PgDn, Insert, Delete, F1 to F12.
- **Where:** `src/tui/keymap.rs` (about 75)
- **Effort:** low. **Tests:** partial.
- **Gaps:** modified arrows lose their modifiers, application cursor mode is
  ignored, no kitty (CSI u) passthrough.

### Agent pane renderer `pty`
A focus-colored border, a title naming the workspace, session and agent, an
optional tab strip, a one-row footer, the terminal drawn with tui-term, and
the real cursor placed only when the pane is focused, alive and showing it.
Resizes the agent to the inner rect each frame.
- **Where:** `src/tui/draw/brain_panel.rs` (about 210)
- **Effort:** low to medium. **Tests:** partial.
- **Note:** the file most sensitive to the ratatui upgrade.

### Numbered tab strip `layout`
" 1 Main  2 foo  3 bar " with the active tab bold; numbers match the Alt+digit
slots. Shown only with more than one tab.
- **Where:** `src/tui/draw/brain_panel.rs` (about 25). **Effort:** low.

### Tabs with stable ids `layout`
Tabs carry an id from a counter that never repeats in a run, so closing a tab
cannot repoint the active one. A permanent first tab, slot lookup, wrapping
cycle.
- **Where:** `src/tui/state/brain/sessions.rs` (about 250),
  `src/tui/state/shell.rs`, `src/tui/app_brain_tab.rs` (about 150 together)
- **Effort:** low to medium. **Tests:** yes.

### Split with a swappable, persisted side `layout`
A 50/50 split whose sides can be swapped from the palette; focus keys are
spatial.
- **Where:** `src/tui/draw/mod.rs`, `src/state/mod.rs` (about 50)
- **Effort:** low. **Tests:** partial.

### Crash-on-arrival detector `pty`
A child counts as started only after it has stayed alive for 5 s. An earlier
death after a resume attempt retries fresh; otherwise it reports a failed
start. Refused resume ids are remembered so it cannot loop.
- **Where:** `src/tui/app_brain/launch/arrival.rs` (about 95)
- **Effort:** low. **Tests:** yes.

### Pane supervision (pattern) `pty`
The permanent first tab relaunches in place without stealing focus; other
tabs close when their child exits; every launch step has a cleanup path.
- **Where:** `src/tui/app_brain/`, `src/tui/app_manual_session/` (about 850)
- **Effort:** high: extract the pattern, not the code.

## Agents

### Input sequences: bracketed paste and settle delays `agent`
A sequence of writes, each with a settle delay. Text is wrapped in bracketed
paste markers with newlines as CR and control characters stripped, so text
cannot break out of the paste. The submit key goes as a separate write 400 ms
later.
- **Where:** `src/agent/input.rs` (about 130)
- **Effort:** low. **Tests:** yes.
- **Lessons:** paste and submit in one write lost the submit. Codex ignores
  Enter within 120 ms of a paste. The older Escape-CR newline knocked vim-mode
  prompts out of insert mode. This is the stronger of the two solutions (see
  dif).

### Agent transport trait `agent`
Spawn, send, snapshot, liveness, shutdown, plus optional screen, resize and
scroll so a headless transport can implement it.
- **Where:** `src/agent/controller/mod.rs` (about 45)
- **Effort:** low. **Tests:** yes.

### Launch spec, environment allowlist, quoting `agent`
Command, working directory, environment and metadata, with values redacted in
debug output. An allowlist of variables to forward, shell quoting that picks
the shorter form, and size guards on the command line.
- **Where:** `src/agent/frontend.rs` (about 420)
- **Effort:** medium (split the spec from brain's request type).
  **Tests:** partial.

### Agent adapter trait `agent`
Turns intents (type text, submit, follow up after the current turn, start a
new session) into input sequences per agent, and reports how completion is
detected and whether a session can resume.
- **Where:** `src/agent/frontend.rs`, `src/agent/session.rs` (about 280)
- **Effort:** medium. **Tests:** yes, one contract suite run against every
  adapter.
- **Note:** `docs/adding-an-agent-frontend.md` is a ready porting checklist;
  its rule is that the installed CLI's `--help` is the source of truth.

### Per-agent input keys `agent`

| Agent | Submit | Follow-up | New session |
| --- | --- | --- | --- |
| Claude Code | CR | paste, then CR | `/new`, CR |
| Codex | CR | paste, then Tab (queues while busy) | `/new`, Tab |
| OpenCode | CR | paste, then CR | `/new`, CR |
| pi | CR | paste, then Alt+Enter as CSI u | `/new`, CR |

- **Where:** `src/agent/{claude,codex,opencode,pi}.rs` (about 15 each)
- **Tests:** yes.

### Launch command builders per agent `agent`
Each agent's flags for session ids, resume, appended instructions and
configuration, plus detection of wrapper commands.
- **Where:** `src/agent/claude*.rs`, `codex*.rs`, `opencode*.rs`, `pi.rs`
  (about 1,500)
- **Effort:** high: ship as reference adapters behind a slimmer request.
- **Note:** brain's own Claude default skips permission prompts. A toolkit
  default must not.

### Agent controller `agent`
One adapter plus one transport: launch with rollback on failure, semantic
input (type, submit, queue after the current turn, forward keys, new session),
idempotent shutdown.
- **Where:** `src/agent/controller/mod.rs` (about 350)
- **Effort:** medium. **Tests:** yes.

### Agent registry `agent`
A table of agent registrations with a test that it is exhaustive.
- **Where:** `src/agent/registry.rs`, `registry/contract.rs` (about 515)
- **Effort:** medium (make it open). **Tests:** yes.

### Resume evidence per agent `agent`
Checks each agent's own session storage before resuming. "Never guess
resume; missing evidence starts fresh."
- **Where:** `src/agent/claude/`, `codex/sessions.rs`, `opencode/session.rs`,
  `pi/sessions.rs` (about 460)
- **Effort:** low to medium; fragile against upstream format changes.

### Command probe in an isolated process group `agent`
Runs a helper with no stdin in its own process group, with bounded output
capture and a timeout that kills the group.
- **Where:** `src/agent/command_probe.rs` (about 190)
- **Effort:** low. **Tests:** partial.

### Agent pane key forwarding `agent`
Any key snaps to the live tail; Enter becomes a semantic submit; a dead tab
closes on Esc or q.
- **Where:** `src/tui/handlers/input.rs` (about 80)
- **Effort:** low to medium. **Tests:** partial.
- **Gap:** without bracketed paste on the host, each pasted newline submits.

### Single-prompt tabs that close on a callback `agent`
A tab runs one prompt; the agent signals completion through a one-time token,
and the tab closes once the signal arrives and its output files exist. "The
agent stopped talking" is not treated as done.
- **Where:** `src/skill_session/`, `src/tui/app_skill_session/` (about 830)
- **Effort:** medium to high. **Tests:** yes.

### Background tabs that never steal focus (rule) `agent`
- **Where:** `src/tui/state/brain/sessions/receiver.rs`

### Lifecycle hooks and progress observation (marginal) `agent`
Idempotent installation of hook commands into an agent's settings, and a small
status file the hooks write so the app can show busy or idle.
- **Where:** `src/agent/hooks.rs`, `src/agent/registry/contract.rs`,
  `src/agent/observation*` (about 1,300)
- **Effort:** high.

### Session store, default agent, command parser (marginal) `agent`
A session store trait, lenient parsing of the default agent setting, and a
check that a configured command is a plain call of a binary.
- **Where:** `src/agent/session.rs`, `src/agent/default_frontend.rs`,
  `src/agent/direct_command.rs` (about 440)

## Commands and keys

### Generic command palette `cmd`
`CommandPalette<A>`: numbered rows with a label, an action and a hint. Every
query word must appear in the row, so typing a number filters to its row.
Wrapping navigation, Ctrl+J/K/P/N, Ctrl+U and Ctrl+W. Returns continue,
confirm or cancel.
- **Where:** `src/tui/palette/model.rs` (about 200)
- **Effort:** low. **Tests:** yes.
- **Drift:** the footer advertises a digit jump that only filters.

### Palette and list modal renderer `cmd`
Title, subtitle, filter line with the real cursor, a scrolling list with a
selection marker and dim hints, a position counter, width fitted to the rows,
height capped at 3/5 of the terminal.
- **Where:** `src/tui/draw_palette.rs` (about 335)
- **Effort:** low. **Tests:** yes.

### Command catalog as the parent set (pattern) `cmd`
One ordered table lists every command in every context; only the wording
changes. Variants encode what a command needs first (nothing, an item, a
path). A plugin registry was rejected so dispatch stays exhaustive.
- **Where:** `src/tui/palette/command/` (about 400), `src/tui/action/global.rs`
- **Effort:** medium. **Tests:** yes.

### Context-aware row wording `cmd`
"Mark T123 as complete" with a target, "Mark a task as complete" without one,
bare wording inside an item's own menu. Toggles name what happens next.
- **Where:** `src/tui/palette/command/naming.rs` (about 240)
- **Effort:** medium. **Tests:** yes.

### Target gating and "which one?" pickers `cmd`
A snapshot taken when the palette opens decides whether a command has a
target. A command without one opens a picker titled by the command; an empty
set shows a flash instead.
- **Where:** `src/tui/palette/context.rs`, `target.rs`,
  `src/tui/app_actions/targets.rs` (about 390)
- **Effort:** medium. **Tests:** yes.

### Label elision `style`
Filenames keep their head and extension around `...`; directory paths keep the
first segment and drop the middle.
- **Where:** `src/tui/palette/command/labels.rs` (about 175)
- **Effort:** low. **Tests:** yes. Counts characters.

### Shortcuts table with parity guards `cmd`
One table drives help and the footer. Tests check that every bound command is
in the palette and pin the short list of navigation-only keys.
- **Where:** `src/tasks/shortcuts.rs`, `shortcuts/table.rs` (about 500)
- **Effort:** low. **Tests:** yes.
- **Lesson:** "parity enforced only by convention eventually breaks".

### Width-aware shortcut footer `cmd`
Chips until the width runs out, always keeping the "all shortcuts" chip, plus
a count chip while digits are pending.
- **Where:** `src/tasks/render/chrome.rs` (about 45)
- **Effort:** low. **Tests:** partial.

### Grouped help modal `cmd`
Groups in a fixed order with a right-aligned key column, wrapped and
scrollable.
- **Where:** `src/tui/draw_help.rs` (about 140)
- **Effort:** low. **Tests:** yes.
- **Bug:** the scroll limit counts logical lines while the text wraps, so the
  last rows can be unreachable.

### Vim count prefix and list motions `cmd`
Digits build a count (capped), the next motion uses it, anything else clears
it. j and k with counts, half pages, g and G.
- **Where:** `src/tui/keymap.rs`, `src/tui/handlers/tasks_view.rs` (about
  250)
- **Effort:** low. **Tests:** yes.

## Navigation and layout

### Single-owner modal host `layout`
One optional overlay enum with open, replace and close transitions and one
routing function; the same enum drives drawing.
- **Where:** `src/tui/overlay/mod.rs` (about 125),
  `src/tui/event_loop/modal_route.rs`
- **Effort:** medium (a `ModalHost<M>`). **Tests:** yes.
- **Gotcha:** opening refuses silently when a modal is already open.

### View cycle and sub-views `layout`
Main views in a wrapping cycle with jump keys; sub-views live inside one main
view and share one effect type.
- **Where:** `src/main_view.rs`, `src/tui/state/shell.rs`,
  `src/tui/search_view.rs`, `src/tui/tree_view.rs` (about 560)
- **Effort:** medium to high. **Tests:** yes.

### Sub-view strip `layout`
"view · a · **b** · c".
- **Where:** `src/tasks/view/mod.rs`, `src/tasks/render/chrome.rs` (about 155)
- **Effort:** low.

### Frame composition `layout`
Error banner and saving row carved off the bottom, the split, the hit-test
rect, the view, the pane, then the one overlay.
- **Where:** `src/tui/draw/mod.rs` (about 150)

### Centered rect `layout`
- **Where:** `src/tui/draw/layout.rs`, with a copy in `src/confirm.rs`.

### Persistent error banner `layout`
A red wrapped band sized to its text that survives focus changes until
dismissed.
- **Where:** `src/tui/draw/error.rs` (40)
- **Effort:** low. **Tests:** yes.

### Three-tier status line `layout`
A flash cleared on the next key, then background status, then a persistent
warning, then the normal hint.
- **Where:** `src/tui/state/status.rs`, `src/tui/status_warning.rs` (about
  280, part app-specific)
- **Effort:** low. **Tests:** yes.

### Logical-to-visual row measuring `layout`
Measures each logical line's wrapped height every frame and keeps prefix sums,
which drive keeping the selection visible, the scroll limit and the selection
band.
- **Where:** `src/tui/state/tasks/interaction.rs`, `src/tui/draw/tasks_panel.rs`
- **Effort:** low to medium. **Tests:** partial.
- **Bug:** a test exercises a test-only copy rather than the production code.

### Variable-height list with selection band and scrollbar `pick`
Paints the selection band, renders the wrapped body, brightens the text in the
band, and draws a scrollbar in a reserved column.
- **Where:** `src/tui/draw/tasks_panel.rs` (about 140), `src/tasks/render/card.rs`
- **Effort:** medium. **Tests:** partial.

## Menus, lists and pickers

### Fuzzy picker `pick`
Every query word must match as a contiguous run (smart case, path-aware),
sorted by group, then score, then original order. Rescoping clears the query;
refreshing keeps it.
- **Where:** `src/picker/mod.rs`, `src/picker/filter.rs` (about 225)
- **Deps:** nucleo-matcher. **Effort:** low. **Tests:** yes.

### Separator-insensitive matching with highlight remap `pick`
Strips `-`, `_` and `.` before matching, then maps match positions back, so
`afloat` finds `ann-afloat` with the right letters highlighted.
- **Where:** `src/picker/haystack.rs` (about 45)
- **Effort:** low. **Tests:** yes.

### Grouped results with headers that cannot be selected `pick`
Header rows ("Label · N") the cursor skips, and scrolling that keeps the
header above the selection visible.
- **Where:** `src/picker/filter.rs`, `src/picker/nav.rs` (about 45)
- **Effort:** low. **Tests:** yes.

### Query editing and list navigation `pick`
Append, Backspace, Ctrl+U, Ctrl+W, arrows, pages, Home and End; each edit
refilters.
- **Where:** `src/picker/nav.rs` (about 100)
- **Effort:** low. **Tests:** yes.
- **Bug:** the footer names keys that are not the bindings.

### Match line and picker chrome `pick`
A selection marker with matched runs merged into one highlighted span; a
header with counts, a query line, separator, empty state and footer.
- **Where:** `src/render/mod.rs`, `src/picker/view.rs` (about 260)
- **Effort:** low. **Tests:** partial.

### Inline filter mode `pick`
`/` filters in place: Backspace on an empty query leaves, Enter keeps the
filter, Esc drops it, Ctrl chords still work.
- **Where:** `src/tui/handlers/tasks_view.rs`
- **Effort:** low to medium. **Tests:** yes.

### Second fuzzy matcher `pick`
A skim-style scored filter for the task list.
- **Where:** `src/tui/state/tasks/filter.rs` (about 30)
- **Note:** brain uses two matchers; the toolkit should pick one.

### Numbered quick-pick `pick`
A digit picks and applies a row; Enter applies the highlighted one. Used for
link and assignee pickers.
- **Where:** `src/tui/draw_modals.rs`, `src/tui/draw_assignee.rs` (about 300)
- **Effort:** low. **Tests:** yes.

### Picker with disabled but visible rows `pick`
Disabled rows are dimmed, struck through and tagged with a reason; they can be
highlighted but not chosen.
- **Where:** `src/tui/draw_modals/session_close.rs`, `session_rename.rs`
  (about 300 together, near-duplicates)
- **Effort:** low. **Tests:** yes.

### Toggle checklist on `/dev/tty` `pick`
A pure checklist (Space toggles, `a` adds new items from free text, Enter
confirms) with a small runner that works outside the main TUI and returns
nothing without a terminal.
- **Where:** `src/personalization/checklist/` (about 310)
- **Effort:** low. **Tests:** yes.

## Tree

### Tree from a flat path list `tree`
Groups paths by parent and creates missing ancestors; directories first,
case-insensitive. Returns the items and a path index from one source so they
cannot drift. No disk reads when opening.
- **Where:** `src/tree/build.rs` (about 220)
- **Deps:** tui-tree-widget. **Effort:** low to medium. **Tests:** yes (20).

### Expand to a target, `../` row, root ceiling `tree`
Opens the tree expanded along a target's ancestors. A `../` row re-roots one
level up, never above a ceiling.
- **Where:** `src/tree/build.rs`, `src/tree/root.rs` (about 80)
- **Effort:** low. **Tests:** yes.

### Tree state `tree`
Explore, collapse, re-root and rebuild, keeping the cursor unless its row is
gone.
- **Where:** `src/tree/mod.rs` (about 230)
- **Effort:** medium. **Tests:** yes (19).

### Tree keys and sibling jumps `tree`
Arrows and hjkl, Space toggles, pages, first and last sibling, `.` for hidden
files. A guard restores the selection whenever a move would empty it.
- **Where:** `src/tree/input.rs`, `src/tree/siblings.rs` (about 210)
- **Effort:** low to medium. **Tests:** yes.
- **Lesson:** tree-widget navigation reads the last render, so tests must
  render once first or they pass vacuously.

### File kinds and row styles `tree`
Directory, runnable, note, data, rendered, archive, other; hidden is a dim
modifier, not a kind. The selected row keeps its own hue, lifted in Oklab.
- **Where:** `src/tree/kind.rs`, `src/render/mod.rs` (about 100)
- **Effort:** low. **Tests:** yes.

### Tree panel renderer `tree`
Header, separator, tree, footer, in the same rect as search so the two swap.
- **Where:** `src/tree/view.rs` (about 80)
- **Effort:** low. **Tests:** yes.

### Directory walker with hidden handling `tree`
Walks a directory, pruning dotted names unless asked, and shortens paths to
`~/`. Hidden files use a second walk so they never affect search ranking.
- **Where:** `src/entry.rs` (about 180)
- **Deps:** walkdir. **Effort:** low. **Tests:** yes.

## Input and dialogs

### Yes/no confirm with a safe default `input`
Destructive confirms are red and default to No, so a stray Enter cancels.
- **Where:** `src/confirm.rs` (about 220)
- **Effort:** low. **Tests:** yes.

### Choice confirm with a bound target `input`
Yes, No and an optional Skip, a success or danger accent, and a dim line
naming the target.
- **Where:** `src/tui/modal_state.rs`, `modals.rs`, `draw_modals.rs` (about
  300)
- **Note:** merge with the confirm above into one dialog.

### Single-line input with horizontal scroll and inline error `input`
A prompt, scrolling by display width, the real cursor, a red error row that
clears on the next edit. Copied three times today.
- **Where:** `src/tui/draw_modals/capture_note.rs`,
  `src/tui/draw_modals/manual_session_rename.rs`, `src/tui/share/draw.rs`
- **Effort:** low. **Tests:** partial.

### Prefilled input with a validator `input`
Enter validates; the modal closes only on success.
- **Where:** `src/tui/handlers/overlay.rs`, `src/manual_session/mod.rs`
- **Effort:** low. **Tests:** yes.

### Multi-line compose `input`
Grows to 12 rows showing the tail; Alt+Enter adds a line, Enter sends.
- **Where:** `src/tui/draw_modals.rs` (about 140)
- **Effort:** low. **Tests:** yes.
- **Gap:** no cursor movement.

### Soft-wrap for input `style`
Breaks on newlines, prefers the last space, splits overlong words.
- **Where:** `src/tui/draw_modals.rs` (about 30)
- **Effort:** low. **Tests:** yes.

### Choice dialog with scrollable details `input`
A scrollable body above a list of choices derived from state; moving never
acts, Enter does. Running, failed and finished screens.
- **Where:** `src/tui/task_removal/` (about 770, part app-specific)
- **Effort:** medium. **Tests:** yes.

### Captive multi-step wizard `input`
Phases with inline errors that keep the current step, a failure that returns
to the previous step without retyping, and Esc at any phase cancelling the
background job.
- **Where:** `src/tui/share/` (about 415)
- **Effort:** medium. **Tests:** yes.

## Viewers

### Scrollable log viewer `view`
Loads once, starts at the bottom, clamps at draw time; j, k, pages, g, G.
- **Where:** `src/tui/logs_view.rs` (about 105)
- **Effort:** low. **Tests:** partial.

### Live log-tail modal `view`
Re-reads its source each frame and follows the tail, with explicit "nothing
running" and "starting" states.
- **Where:** `src/tui/draw_sync_log.rs` (about 75)
- **Effort:** low. **Tests:** yes.
- **Bugs:** after `G`, `k` looks stuck; file reads happen in draw.

### Bounded log-tail export `view`
Reads at most the last 256 KiB and 2,000 lines, realigned to a line and a
UTF-8 boundary.
- **Where:** `src/logging/export.rs` (about 55)
- **Effort:** low. **Tests:** yes.

### Small markdown renderer `view`
Bold, italic, code, headings, bullets and quotes, one output line per source
line.
- **Where:** `src/tasks/render/markdown.rs` (about 155)
- **Effort:** low. **Tests:** yes.

### Links in text and OS actions `view`
Extracts URLs, opens directly when there is one, offers a picker when there
are several. OS actions: open, reveal, edit in a new terminal tab, copy, move
to trash (never delete).
- **Where:** `src/tui/links.rs`, `src/tui/state/tasks/links.rs`,
  `src/open_target.rs` (about 400)
- **Effort:** low (links), medium (cross-platform actions). **Tests:** yes.
- **Gap:** macOS only.

## Style

### Oklab color math `style`
sRGB to Oklab and back, and a perceptual `lighten` that leaves non-RGB colors
alone.
- **Where:** `src/render/oklab.rs` (about 160)
- **Effort:** low. **Tests:** yes.
- **Lessons:** 0.125 lift was measured; 0.18 clipped to near white. A cruder
  sRGB brighten in `src/tui/draw/layout.rs` duplicates it.

### Dark palette and style helpers `style`
Text tiers, accents, match highlight, selection background.
- **Where:** `src/render/mod.rs`, `src/tasks/render/style.rs` (duplicated)

### Semantic theme for command-line output (marginal) `style`
- **Where:** `src/theme.rs` (about 115)

### Text helpers (marginal) `style`
Truncation with `…`, relative dates, stripping ANSI codes from captured
output.
- **Where:** `src/tasks/render/style.rs`, `src/sync/progress/mod.rs`

## Background work

### Job coordinator with worker lanes and optimistic updates `task`
Jobs with ids that settle independently of the dialog that started them;
accepted jobs block quit; duplicate requests are refused. Serial worker lanes
with a retry when a worker dies. Pending changes are overlaid on the last
known data until a newer snapshot confirms them, and stale snapshots are
rejected.
- **Where:** `src/tui/task_mutation/` (about 450 generic of 680)
- **Effort:** medium. **Tests:** yes.

### Cancel-on-drop subprocess worker `task`
Runs a child in its own process group, streams its stderr lines, caps its
output, and kills the group on drop.
- **Where:** `src/tui/share/worker.rs` (about 135)
- **Effort:** low (Unix only). **Tests:** yes.

### Detached process with a progress marker, and a debouncer `task`
Long jobs run as detached children writing a log and a status marker the TUI
polls. A file-watch debouncer is a pure state machine with an injected clock.
- **Where:** `src/sync/current.rs`, `src/sync/watch.rs`, `src/tui/app_sync.rs`
- **Effort:** low (debouncer), medium (the rest). **Tests:** yes.
- **Lesson:** running this work on threads bled stderr over the TUI and
  orphaned children on quit.

### Shell runner seam (marginal) `task`
A trait for side-effecting shell actions that tests replace.
- **Where:** `src/tui/shell.rs` (about 60)

## Bugs and drift found while reading

1. The iTerm2 wheel bug described under the terminal guard.
2. No panic hook.
3. Paste is not bracketed on the host, so a multi-line paste into the agent
   pane submits at each newline.
4. `docs/decisions.md` mentions a mode (`?1007l`) the code does not send, an
   older input pacing, and filter policies that no longer exist.
5. The help modal's scroll limit ignores wrapping.
6. The live log modal's `k` appears stuck after `G`.
7. Footer hints name the wrong keys in two places.
8. Width math mixes characters and terminal cells.
