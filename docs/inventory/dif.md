# Inventory: dif

`dif` is a two-pane review shell: a diff server log and markdown views on the
left, a coding agent (Claude Code or Codex) in an embedded terminal on the
right.

- **Repository:** `AvandarLabs/avandar`, under
  `agent-skills/public-skills/skills/diff-review/scripts/dif/`. Paths below are
  relative to that directory.
- **Versions:** edition 2024, Rust 1.89, ratatui 0.30, crossterm 0.29, vt100
  0.16 (held back until tui-term supports a newer one), tui-term 0.3,
  portable-pty 0.9, pulldown-cmark 0.13.
- **Event loop:** synchronous and single-threaded. Each pass draws, services
  background work, then polls crossterm for 50 ms and reads one event. Worker
  threads (three per embedded terminal, file and HTTP watchers, a local HTTP
  server) hand results back through shared slots the loop reads, never through
  a channel into the loop.
- **State shape:** one mutable `App` with `impl` blocks split across files.
  Modals are `Option` fields, and key routing is a fixed priority chain: modal,
  help, palette, global chords, focused pane. Drawing takes `&mut App`: it
  records pane areas for mouse hit-testing, resizes the embedded terminals and
  feeds rendered lines back to the document view.
- **Theme:** none. RGB colors are repeated across about eight files and
  `NO_COLOR` is not honored in the TUI.

Line counts are production lines, without inline tests, and approximate.
Categories are the ones defined in [`README.md`](README.md).

## Terminal

### Terminal setup and teardown `term`
Raw mode, alternate screen, mouse modes on the way in; the reverse on the way
out, even when the loop returns an error.
- **Where:** `src/tui/mod.rs` (about 30)
- **Generic:** all of it. **App-specific:** two startup and cleanup calls.
- **Effort:** low. **Tests:** none.
- **Lessons:** there is no panic hook, so a panic leaves raw mode and mouse
  reporting on. Setup writes mouse-mode bytes itself rather than calling
  crossterm's `EnableMouseCapture` (see the mouse modes below).

### Event loop with layered key capture `cmd`
Routes each key through modal capture, then global Ctrl and Alt chords (so the
embedded agent never sees them), then the focused pane.
- **Where:** `src/tui/event_loop.rs` (about 250)
- **Generic:** the capture order and "intercept before forwarding to the
  embedded terminal". **App-specific:** the chords and the `App` methods.
- **Effort:** medium. **Tests:** partial.
- **Lessons:** bracketed paste is never turned on, so a paste reaches the
  agent as keystrokes and an embedded newline submits early.

### Clipboard: pbcopy, OSC 52 and tmux passthrough `term`
Copies with `pbcopy`. Over SSH it also emits OSC 52 so the user's own terminal
sets the clipboard, and inside tmux it sends both the plain and the
DCS-wrapped form. Carries its own base64 to avoid a dependency.
- **Where:** `src/clipboard.rs` (about 85)
- **Generic:** all of it. **Effort:** low. **Tests:** partial (base64, both
  OSC 52 forms).
- **Lessons:** the local route is macOS-only, so plain Linux copy fails.
  OSC 52 is written to stdout while ratatui owns it.

## Mouse

### Mouse modes, pane hit-testing, pointer clamping `mouse`
Turns on modes 1000, 1002 and 1006 and nothing else. Maps a cell to the pane
under it, clamps a pointer into a pane, and reports whether a drag overshot
the top or bottom. The wheel scrolls the pane under the pointer, focused or
not.
- **Where:** `src/tui/mouse.rs` (about 115)
- **Generic:** the mode bytes, clamping, overshoot, hit-testing.
  **App-specific:** `PaneAreas` hard-codes two panes.
- **Deps:** crossterm, ratatui. **Effort:** low. **Tests:** yes, including
  "never 1003, never a reset".
- **Lessons:** iTerm2 keeps modes 1000, 1002 and 1003 in one slot, and
  resetting any of them switches reporting off, which once silently killed the
  wheel. Mode 1003 stays off so iTerm2's Cmd-hover and Cmd-click keep working.
  Option-drag (iTerm2) and Fn-drag (Terminal.app) fall through to the
  terminal's own selection. Mouse events are never forwarded into the
  embedded terminal.

### Multi-click tracker, word and line finders `mouse`
Counts 1, 2, 3 presses on the same cell within 500 ms. Finds a word on a
terminal screen (keeping wide characters whole) or in a plain line, with a
word-character set tuned for paths and `file.rs:12`. Extends a line across
soft-wrapped rows.
- **Where:** `src/tui/multi_click.rs` (about 115)
- **Generic:** all of it; the word characters should be a setting.
- **Deps:** vt100. **Effort:** low. **Tests:** yes, on a real vt100 screen.
- **Lessons:** the time is passed in, which is what makes it testable.

### Selection model, highlight and text extraction `mouse`
A selection is an anchor, a head and a unit (cell, word, line) in content
coordinates, so it stays on its text while the pane scrolls. Paints a
background over the covered cells. Copies from a vt100 screen across
scrollback (stepping the offset so a selection taller than the pane copies in
full, joining soft-wrapped rows, keeping wide characters whole), or from plain
rendered lines.
- **Where:** `src/tui/selection.rs` (about 210)
- **Generic:** all but two fields that store dif's pane and view types.
- **Deps:** ratatui, vt100. **Effort:** low. **Tests:** yes, extensive.

### Drag-to-copy controller `mouse`
Glue between the events and the selection: starts a cell, word or line
selection, follows drags, scrolls a terminal pane while a drag overshoots,
copies on release and shows "Copied N characters".
- **Where:** `src/tui/app/select.rs` (about 190)
- **Generic:** the logic. **App-specific:** it is an `impl App`. Needs a
  trait over selectable surfaces.
- **Effort:** medium. **Tests:** none directly.

## Embedded terminal

### Embedded terminal pane `pty`
Runs a child under the user's shell in a pseudo-terminal, feeding a vt100
parser with 10,000 rows of scrollback through reader, writer and waiter
threads. Sends input, resizes, scrolls the scrollback, reports liveness, kills
the child on drop, writes the app's own lines into the screen, and respawns a
new child into the same parser so history continues across restarts.
- **Where:** `src/pty_pane.rs` (about 310)
- **Generic:** all of it. Copied from brain and since diverged.
- **Deps:** portable-pty, vt100. **Effort:** low to medium. **Tests:** yes,
  against a real pseudo-terminal.
- **Lessons:** drop the child's end right after spawning so the child sees
  end-of-file. The child should be `exec`ed so killing it reaches the real
  process. Late output from a replaced child can interleave.

### Embedded terminal renderer `pty`
Resizes the pane to its area, draws the screen with tui-term, and places the
real cursor at the child's cursor when the pane is focused and alive.
- **Where:** `src/tui/draw.rs` (about 35 lines of it)
- **Deps:** tui-term, vt100. **Effort:** low. **Tests:** none.
- **Lessons:** resizing inside draw means the first frame corrects a
  placeholder 24x80 size.

### Key events to bytes `pty`
Encodes a crossterm key as xterm bytes: control codes, Alt as an Escape
prefix, Enter, Tab, Backtab, Backspace, Delete, Home, End, PgUp, PgDn, arrows.
- **Where:** `src/tui/keymap.rs` (about 50)
- **Generic:** all of it. **Effort:** low. **Tests:** yes.
- **Gaps:** no function keys (the header claims them), no modified arrows, no
  Insert, no Ctrl with non-letters, and application cursor mode is ignored.

### Exited-child placeholder `pty`
Drops a pane once its child exits and draws "session ended" in its place.
- **Where:** `src/tui/app/llm.rs`, `src/tui/draw.rs` (about 30)
- **Effort:** low. **Tests:** partial.

### Activity log written into a pane `pty`
Writes the app's colored status lines into an embedded terminal's screen, so a
server console doubles as an activity log and restarts are logged inline.
- **Where:** call sites across `src/tui/app/` and `src/tui/startup/`
- **Effort:** low. **Tests:** through the respawn test.

### Restart a child in place `pty`
Writes a status line, kills the child, waits for its resources, and respawns
into the same pane, keeping size and scrollback.
- **Where:** `src/tui/app/review_surface.rs` (about 80 of it)
- **Effort:** medium. **Tests:** none.
- **Lessons:** waiting for a port to free blocks the UI for up to 2 s.

## Agents

### Typing a prompt into an agent `agent`
Writes the prompt with newlines encoded as Alt+Enter, then sends the submit key
in a separate write 200 ms later: Enter for Claude Code, Tab for Codex.
- **Where:** `src/tui/keymap.rs` (about 60)
- **Generic:** all of it, as an agent profile. **Effort:** low.
  **Tests:** yes, on a real pseudo-terminal.
- **Lessons:** Claude Code treats a newline arriving in the same burst as
  pasted text, so the submit has to be its own write. Several prompts sent in
  one pass write every body before the first submit lands; a per-pane queue
  would serialize them. brain's version is stronger (see brain).

### Agent launch and session resume `agent`
An agent kind with its resume and fresh-session flags, a resume-or-fresh plan,
the launch command line with POSIX shell quoting, and the path Claude Code
stores a session's transcript under, used to decide whether a resume is
possible.
- **Where:** `src/session.rs` (about 140), `src/tui/startup/llm_command.rs`
  (about 130)
- **Generic:** the profile, the plan, quoting. **App-specific:** prompts.
- **Deps:** uuid. **Effort:** low to medium. **Tests:** yes, thorough.
- **Lessons:** Codex cannot take a caller-chosen session id, so fresh Codex
  sessions are not resumable.

### Message queue into the agent `agent`
Turns new items (here, review comments) into prompts exactly once, after a
baseline taken at launch. Three delivery modes: one prompt per item, hold and
count, or one batched prompt. Items held across a restart are persisted so the
next baseline does not swallow them. A reply watcher reports each new agent
reply once.
- **Where:** `src/inject/` (about 590 across six files)
- **Generic:** the baseline-then-once dispatcher, the modes, the persisted
  held set, the reply watcher. **App-specific:** the comment snapshot type and
  prompt wording.
- **Effort:** medium. **Tests:** yes, thorough.

### Local control endpoint `agent`
A localhost HTTP mailbox that lets an agent or skill signal the running TUI,
with an accept gate. The UI polls a take-once slot.
- **Where:** `src/tui/control.rs` (about 135)
- **Deps:** tiny_http. **Effort:** low. **Tests:** yes, live HTTP.

### Live-session metadata file (marginal) `agent`
Writes a JSON record of ports, pid and paths so other tools can find the
running instance.
- **Where:** `src/tui/session_meta.rs` (about 45). **Effort:** low.

## Commands and keys

### Command palette state `cmd`
A static ordered table of commands with stable row numbers, so a typed digit
always means the same command. Case-insensitive substring filter over the
number and label, wrapping selection, and a dim shortcut hint per row.
- **Where:** `src/tui/palette.rs` (about 200)
- **Generic:** the state machine. **App-specific:** the command enum.
- **Effort:** low. **Tests:** yes.
- **Lessons:** substring, not fuzzy. No scrolling for long lists.

### Command palette renderer `cmd`
A centered box at most 60 columns wide: title, filter line with the real
cursor, separator, numbered rows with a selection marker and dim hints, a
footer of key hints.
- **Where:** `src/tui/draw_palette.rs` (about 150)
- **Effort:** low. **Tests:** partial.

### Shortcuts table, help modal, statusline chips `cmd`
One table of shortcuts, grouped, drives both the compact statusline and the
help modal.
- **Where:** `src/tui/shortcuts.rs` (about 245, mostly data),
  `src/tui/draw_help.rs` (75)
- **Effort:** low. **Tests:** partial.
- **Lessons:** the table is separate from the key handling, so they can drift,
  and one description already has. The help modal neither scrolls nor wraps.

### Vim motion grammar `cmd`
Accumulates a count and the `gg` double-tap, and emits motions: up, down,
left, right, word forward and back, half page, top, bottom, open.
- **Where:** `src/tui/vim.rs` (about 110)
- **Effort:** low. **Tests:** yes.

## Navigation and layout

### Two-pane shell with focus `layout`
A fill area plus a one-row statusline, split 50/50. Focus-aware borders, pane
areas recorded for hit-testing, overlays drawn last.
- **Where:** `src/tui/draw.rs` (about 120), `src/tui/app/mod.rs`
- **Generic:** split panes with focus. **App-specific:** two fixed panes.
- **Effort:** medium. **Tests:** partial.

### View switcher and tab strip `layout`
A fixed cycle of views with wrapping next and previous, drawn as a one-line
strip with the active view highlighted.
- **Where:** `src/tui/main_diff_view.rs` (about 50), `src/tui/draw.rs`
- **Effort:** low. **Tests:** yes.
- **Gaps:** tabs are not clickable.

### Statusline with a transient notice `layout`
Shortcut chips on the bottom row, replaced by a notice for 3 s. Expiry is
checked at draw time.
- **Where:** `src/tui/draw.rs` (about 45)
- **Effort:** low. **Tests:** none.

### Centered modal rect `layout`
Centers a fixed-size rect inside an area, clamped to stay inside it.
- **Where:** `src/tui/modal_layout.rs` (about 20)
- **Effort:** low. **Tests:** yes. The same function exists in brain twice
  and in quickstarter.

### Per-pane scroll dispatch `layout`
One scroll API over pane kinds: a document cursor or a terminal's
scrollback. Half-page steps are half the visible rows.
- **Where:** `src/tui/app/scroll.rs` (150)
- **Effort:** medium (needs a trait). **Tests:** partial.

## Input and dialogs

### Yes/no confirm modal `input`
Pure state and a pure key map (`y`, `n`, Esc, Enter, arrows, Tab; everything
else swallowed so nothing leaks into the agent), drawn as a centered box with
a wrapped body and buttons where the selected one is filled and bold.
- **Where:** `src/tui/start_review/` (about 300 generic of 430)
- **Effort:** low. **Tests:** yes.
- **Bug:** Left selects No and Right selects Yes, the reverse of the button
  order on screen.

## Viewers

### Markdown to ratatui text `view`
Renders markdown at a width: colored headings, bold, italic, strikethrough,
links, inline and fenced code, nested lists with hanging indents, task
markers, rules, and tables.
- **Where:** `src/tui/markdown/mod.rs` (about 280)
- **Deps:** pulldown-cmark. **Effort:** low to medium. **Tests:** yes.
- **Gaps:** widths count characters, not terminal cells. No block quotes,
  images or footnotes. Re-rendered every frame. The design note says a custom
  renderer was chosen over a text-editor widget because those render only
  plain text.

### Style stack and style-preserving word wrap `style`
Composes nested emphasis, and wraps styled spans at whitespace keeping each
word's style.
- **Where:** `src/tui/markdown/inline.rs` (about 100)
- **Effort:** low. **Tests:** yes. Reusable outside markdown.

### Box-drawn table fitted to a width `view`
Natural column widths, shrunk proportionally to fit with `…` truncation, with
column alignment and full borders.
- **Where:** `src/tui/markdown/table.rs` (about 175)
- **Effort:** low. **Tests:** yes.

### Read-only document view with a cursor `view`
A styled pager whose cursor is the source of truth: each frame clamps the
cursor to the rendered lines and moves the scroll minimally to keep it in
view. Motions by character, word, half page, top and bottom. Re-reads its
file and reports whether it changed.
- **Where:** `src/tui/guide.rs` (about 280), drawn in `src/tui/draw.rs`
- **Effort:** low. **Tests:** yes.
- **Gaps:** the column is a character index painted on a cell grid. The file
  is polled about 20 times a second.

### Open what the cursor is on `view`
Takes the token under the cursor, trims surrounding punctuation, and
classifies it as a URL or a path with an optional line (`:12`, `:L12`,
`:L1-L6`). Opens URLs in the browser and text files in an editor in a new
tmux window or terminal tab, so the running TUI is never replaced.
- **Where:** `src/tui/open_target.rs` (about 190)
- **Generic:** token and target parsing, the URL opener. **App-specific:** the
  editor strategy.
- **Effort:** low (parsing), medium (pluggable openers). **Tests:** parsing.

## Background work

### Debounced change alert with attribution `task`
Warns once a watched value has settled for 3 s after changing, re-arms for
each new settled value, and attributes the change to the agent when a prompt
was sent recently.
- **Where:** `src/tui/change_alert.rs` (about 60), `src/tui/app/llm.rs`
- **Effort:** low to medium. **Tests:** partial (reads the clock directly).

### Latest-value watcher thread `task`
Computes a value every interval on a thread, publishes the latest one, and
stops and joins on drop.
- **Where:** `src/git_watcher.rs` (about 90), `src/difit/poller.rs`
- **Effort:** low. **Tests:** partial.

## Out of scope

- Port picking, slugs, atomic file writes: small utilities, not UI
  (`src/difit/server.rs`, `src/slug.rs`, `src/difit/transcript.rs`).
- The config subcommand with a `/dev/tty` prompt (`src/config.rs`,
  `src/cli.rs`): command-line plumbing.
- The browser web shell (`src/web/`): a separate surface, not ratatui.

## Bugs and drift found while reading

1. No panic hook: a panic leaves the terminal in raw mode with mouse
   reporting on.
2. `docs/features.md` says the app starts focused on the diff view; the code
   starts on the agent pane.
3. The confirm modal's arrow keys are reversed relative to the buttons.
4. The key encoder's header promises function keys it does not encode.
5. Several prompts sent in one pass can interleave before their submits.
6. Clipboard, the default opener and the editor launcher are macOS-only.
7. Width math counts characters, not terminal cells.
