# `ratatui-mouse` spec

Mouse support for ratatui apps, including apps that embed terminals: clicks,
double and triple clicks, drag to select and copy across panes and
scrollback, timed scrolling at the edges, and pass-through to the programs
inside panes. It keeps the host terminal's own mouse features working, and
every action has a keyboard equivalent. It uses crossterm.

This is the first building block to be built. It follows
[`architecture.md`](architecture.md) for conventions, and
[`../terminals.md`](../terminals.md) is the source of every claim here about
what a terminal does. Its first user is `dif` (`AvandarLabs/avandar`).

## Goals

1. **One model for every clickable or selectable thing.** The app registers
   areas with targets of its own type. The crate turns raw mouse events into
   outcomes about those targets.
2. **Selection that works on app-held text and inside embedded terminals**,
   through one small trait: what the emulator shows, its scrollback, wrapped
   lines and wide characters.
3. **Pass-through.** A program in a pane that asked for the mouse gets it, in
   the format it asked for. A full-screen program that did not ask gets arrow
   keys from the wheel.
4. **The host terminal keeps working.** Hover reporting stays off, so the
   terminal's own hover and link features are untouched, and a mouse mode is
   never reset during a session.
5. **Keyboard first.** Stock tmux forwards no mouse events, so everything the
   mouse does is reachable from keys.
6. **A part, not a framework.** The crate owns no loop, thread, clipboard,
   focus or key binding of the app. It reports and the app decides.

## Non-goals

- Hover: nothing reacts to the pointer moving with no button down.
- Coordinates finer than a cell.
- Rectangular (block) selection.
- Scrollbars. ratatui's `Scrollbar` and `tui-scrollbar` exist.
- Resizing panes. A drag on a divider is reported like any other drag, and
  what it means is the app's.
- Dragging something from one target and dropping it on another.
- A clipboard. The crate hands the app the text.
- Parsing raw input bytes. crossterm does that.
- Backends other than crossterm, and emulators other than `vt100` (the trait
  leaves room for one).

## What makes it different

Checked in November 2026. None of these is a dependency.

| Crate | What it covers | What is missing for this job |
| --- | --- | --- |
| `tui-panel-select` 0.2 | Drag to select and copy inside one panel, with timer-driven scrolling | It selects only text it stores and wraps itself, so not a terminal pane or its scrollback. No double or triple click. It turns the mouse on with crossterm's `EnableMouseCapture`, which leaves hover reporting on. |
| `ratatui-interact` 0.5 | A registry of clickable areas, focus, widgets | No selection, no multi-click, no drag tracking |
| `rat-event` 2.1 | Double-click and drag detection per widget | No triple click, selection or edge scrolling, and it belongs to its own framework |
| `tui-scrollbar` 0.2 | Draggable scrollbars | Scrollbars only |
| `terminput` 0.5 | Encoding input events as escape sequences | One format, and no decision about when to forward |
| `alacritty_terminal` | Cell, word and line selection | Only over its own emulator |
| `tui-term` | Drawing a `vt100` screen | No mouse and no selection |

Three things together are what no existing crate does: selecting inside an
embedded terminal, passing the mouse into it, and leaving the host terminal's
own mouse features alone.

Some of the gesture ideas below (the press owning the gesture, explicit
selection stages, timed edge scrolling, a redraw budget) are also found in
other projects, among them herdr (`herdrdev/herdr`) and `tui-panel-select`.
This crate reimplements ideas and copies no code from either. Whoever builds
it works from this spec and from terminal documentation, not from their
source.

## Dependencies and features

| Dependency | When | Why |
| --- | --- | --- |
| `ratatui` | always | `Rect`, `Position`, `Buffer`, `Style`, `CellWidth` |
| `crossterm` | always | The event types, and the console API on Windows |
| `unicode-segmentation` | always | Grapheme clusters, to count columns in app-held text. Already in ratatui's own tree. |
| `vt100` | feature `vt100` | The surface over an emulator screen |

No other dependency and no default feature. The `vt100` feature exposes
`vt100` types in two constructors, so a new major version of `vt100` is a
breaking change of that feature.

## Modules

One module per file, each within the size budget in `docs/rules/rust.md`.

| Module | Holds |
| --- | --- |
| `modes` | The sequences, the functions that write them, the guard, the platform split |
| `hit` | `HitAreas<T>` and clamping a pointer into an area |
| `clicks` | `ClickCounter<T>` |
| `content` | `ContentPoint`, `ContentRange` and their conversions |
| `surface` | `SelectableSurface`, `SurfaceSet<T>`, `NoSurfaces`, `LineSurface` |
| `words` | Word, character and text finders over one line, in cells |
| `selection` | `Selection<T>`, units and stages |
| `highlight` | `HighlightRects` |
| `passthrough` | `PaneMouseRequest`, the routing decisions, the encoders |
| `select_mode` | `SelectionCommand`, the default key map, cursor movement |
| `controller` | `MouseController<T>`: the gesture state machine, split into submodules by gesture kind |
| `settings` | `MouseSettings`, `WordCharacters` |
| `outcome` | `MouseReport<T>`, `MouseOutcome<T>`, `PointerPress` |
| `error` | `MouseError` |
| `vt100` | `Vt100Surface`, `PaneMouseRequest::from_screen` |

## Mouse modes

Each rule is backed by a measurement in `terminals.md`, taken in iTerm2 3.7.7,
xterm.js 6.0.0 and tmux 3.8.

| # | Rule | Why |
| --- | --- | --- |
| M1 | Turning the mouse on writes exactly `CSI ?1000h`, `CSI ?1002h`, `CSI ?1006h`, in that order | Modes 1000, 1002 and 1003 are one slot with three values, so the last one written is the one in force. After this sequence all three terminals report button-motion tracking (1002) and SGR coordinates (1006). |
| M2 | Mode 1003 is never written | It reports every pointer move. It floods the app, and it takes the pointer away from the terminal's own hover and link handling. |
| M3 | crossterm's `EnableMouseCapture` is never used to write the sequence | Its sequence ends by setting 1003, which becomes the value in the slot. |
| M4 | No mouse mode is reset between setup and teardown | Resetting any of 1000, 1002 or 1003, even one that was never set, switches reporting off entirely in all three. |
| M5 | Teardown writes `CSI ?9l`, `?1000l`, `?1002l`, `?1003l`, `?1005l`, `?1006l`, `?1015l`, `?1016l` | This is the sequence measured to leave every tracking mode and every coordinate format off in all three. It also clears what a buggy earlier setup left behind. |
| M6 | Pausing reporting (to step aside for a child program) is a full teardown followed later by a full setup | It is the only pause M4 allows. |
| M7 | Nothing asks the terminal whether the mouse is on by asking about mode 1000 | After M1 the terminals report 1002 as set and 1000 as reset. A check has to read 1000, 1002 and 1003 together. Only the test programs ask. |
| M8 | Setup and teardown go to the stream the app draws on | A sequence written to stdout while the UI is on stderr or the controlling terminal can be redirected away. |

Teardown runs on every exit path. The crate provides the pieces: a guard that
tears down on drop, a function for an explicit teardown that reports its
error, and the teardown bytes for a panic hook. In the toolkit,
`avandar-tui-term`'s guard and panic hook call them in the right order with
the other terminal modes. An app that uses this crate alone installs its own
hook.

### tmux

tmux accepts the sequences under the same one-slot rule. With its stock
configuration it forwards no mouse events to the app, so the app receives
nothing and loses nothing: see [Keyboard equivalents](#keyboard-equivalents).
With `mouse on` it forwards events once the app has asked for them. Both
halves are marked to verify with injected events in `terminals.md`, and the
tmux layer of the test plan is where they are proved.

### Windows

On Windows crossterm reads the console's input records, not bytes, and mouse
events arrive only when mouse input is on in the console's input mode. So on
Windows the crate writes no sequence. Setup turns mouse input on through
crossterm, which is the one maintained, safe way to set that mode, and
teardown restores the mode. The pseudo-console then asks the hosting terminal
for mouse reports itself.

- The crossterm command that sets the console mode is `EnableMouseCapture`.
  On Windows it writes no sequence at all, so M3 is not broken: M3 is about
  the sequence that command writes on macOS and Linux.
- The functions and the guard have the same signatures on every platform.
  The platform split is one file in `modes`.
- Pointer moves with no button, if the pseudo-console delivers them, are
  ignored like every other hover event.
- Which tracking mode the pseudo-console asks xterm.js for is not measured
  yet. See [Open questions](#open-questions).

## Hit areas

`HitAreas<T>` is a list of rectangles, each with a target of the app's own
type. It is rebuilt on every frame.

- The app owns one `HitAreas<T>`. Drawing clears it and pushes an area for
  each interactive thing as it is drawn: pane contents, tabs, buttons, rows,
  dividers, borders that should react.
- **Later pushes are on top.** A modal pushed last covers what is under it.
  To make everything outside a modal inert, the app pushes one area over the
  whole screen with a target of its own (a backdrop) before the modal's
  areas.
- The area pushed for a selectable surface is the rectangle its content is
  drawn in, inside any border. The crate maps a pointer to content through
  that rectangle.
- A target is `Clone + PartialEq`. It is usually a small enum:

```rust
enum Target { Guide, AgentPane, Tab(TabId), Divider, Backdrop }
```

- Events are matched against the areas of the last frame drawn, which is the
  frame the user was looking at when they clicked.
- An empty rectangle is never hit.

## The gesture model

### The press owns the gesture

A gesture starts when a button goes down and ends when it comes up. The
target under the pointer at the press is the gesture's **owner**, and every
drag and the release belong to the owner, wherever the pointer goes.

- The kind of gesture is fixed at the press:

| The press landed on | And | Gesture |
| --- | --- | --- |
| A target with a surface, in a pane whose program asked for the mouse | No selection modifier held, and select mode is not on for it | **Pass-through**: the press, its drags and its release are encoded for the program |
| A target with a surface | The left button, and the row above does not apply | **Selection** |
| Any other target, or any other button | | **Plain**: the app is told about the press, drags and release |
| No target | | No gesture. A finished selection is cleared. |

- During a gesture the owner's area is looked up again on each event. While
  the pointer is outside it, the position is clamped to the nearest cell
  inside. If the owner has no area in the current frame, the gesture is
  cancelled.
- A second press while a gesture is active means a release was lost. The
  active gesture is cancelled without a click or a copy, and the new press
  starts a gesture.
- Losing focus or a resize cancels the active gesture.
- A pointer move with no button down is ignored, on every platform.
- Every press, whatever its kind, is reported to the app as the pressed
  target, so the app can move focus.

### Multi-click

A left press continues the previous one when it is on the same target, within
the multi-click interval of it, and at most one cell away from it in each
direction. The count runs 1, 2, 3 and then starts again at 1. A press that
turned into a drag ends the run.

One cell of tolerance matters: a hand moves between clicks, and a pointer on
a cell border reports either side.

### Selection stages

A selection has a unit and a stage.

| Unit | Started by | Covers |
| --- | --- | --- |
| `Cell` | A single press, then a drag | From the character at the press to the character under the pointer |
| `Word` | A double click | The word under the pointer, or the single character when it is not on a word |
| `Line` | A triple click | The whole line, across its soft-wrapped rows |

| Stage | Meaning |
| --- | --- |
| `Pressed` | The button is down and the pointer has not moved. A single press selects nothing yet. A double or triple click has its word or line selected already. |
| `Dragging` | A drag event arrived for this gesture. It stays `Dragging` even if the pointer returns to the cell it started on: one selected character is a selection, not a click. |
| `Done` | The button came up over a selection. It stays highlighted until something clears it. |

What the release does:

| Released in | With | Result |
| --- | --- | --- |
| `Pressed` | A single press | A click. Nothing is selected. |
| `Pressed` | A double or triple click | The word or line is finished |
| `Dragging` | | The selection is finished |

A finished selection whose text is empty or only blanks is dropped. Otherwise
it moves to `Done`, and with copy on release on, the crate returns the text
for the app to copy.

### Word and line drags

After a double click, dragging extends by whole words: the selection is the
word that was double-clicked joined with the word under the pointer. After a
triple click, dragging extends by whole lines the same way. The original word
or line always stays selected, whichever side the pointer goes.

### Extending

A press with the selection modifier held, on the target that already has a
finished selection, moves that selection's far end to the pressed point
instead of starting a new one.

### Timed scrolling at the edges

While a selection gesture's pointer is past the top or bottom of its owner's
area, the surface scrolls on a timer, so holding still keeps scrolling.

- The crate never starts a thread. It keeps a deadline. The app reads it with
  `next_deadline()`, waits no longer than that, and calls `tick(now)` on each
  pass of its loop.
- The drag event that first leaves the area sets the deadline to now. Each
  due tick scrolls the surface, moves the selection's end to the content now
  under the clamped pointer, and sets the next deadline one auto-scroll
  interval later.
- A tick scrolls as many rows as the pointer is past the edge, at least one
  and at most the auto-scroll maximum. Further out is faster.
- Drag events while outside only update the distance. They never scroll by
  themselves, so a shaking hand does not speed it up.
- When the surface cannot scroll further, the deadline is cleared until the
  next drag event.
- A pane that touches the edge of the terminal leaves the pointer nowhere to
  go past it. The edge margin setting counts the outermost rows of the area
  as past the edge for such layouts.

### The wheel

| When | The wheel |
| --- | --- |
| A selection gesture is active | Scrolls the owner's surface by the wheel step and moves the selection's end to the content now under the pointer |
| A pass-through gesture is active | Is forwarded to the owner's program |
| No gesture, over a pane | Follows [Pass-through](#pass-through-into-panes) |
| No gesture, over any other target | Is reported to the app with the target and the rows, and the app scrolls or moves a cursor |
| No target under the pointer | Is ignored |

The wheel never needs focus: it goes to what is under the pointer. A finished
selection stays on its text while its surface scrolls, because it is stored in
content coordinates.

The rule for who scrolls: **the crate scrolls a surface only while a
selection is being made on it** (edge scrolling, the wheel during a drag,
select mode). Otherwise it reports the wheel and the app scrolls.

### Redraw budget

A fast drag produces far more events than frames worth drawing. Every report
carries `is_redraw_due`.

- When something visible changed and at least one redraw interval has passed
  since the crate last said so, the report says a redraw is due.
- Otherwise the change is remembered, `next_deadline()` includes the moment
  the interval ends, and the `tick` at that moment says a redraw is due.
- An app that draws on every pass can ignore the flag. An app that draws only
  when asked gets at most one redraw per interval from the mouse.

## Selectable surfaces

A surface is anything text can be selected in. The app hands the crate its
surfaces through `SurfaceSet<T>`, which returns the surface behind a target.

### Content coordinates

A selection is stored as content points, never as screen cells, so it stays
on its text while the surface scrolls.

- A `ContentPoint` is a line and a column. The column is in **cells**, not
  characters, counted from the start of the content row.
- The surface defines its line numbers and reports its **origin**: the
  content point shown in the top-left cell of its area. A screen cell maps to
  `origin + (row, column)` within the area.
- A range is inclusive at both ends and ordered.

### What a surface does

| Method | Contract |
| --- | --- |
| `origin` | The content point at the top-left of the viewport, now |
| `line_bounds` | The first and last content line that exist |
| `character_at` | The cells of the character at a point: two for a wide character, from either half |
| `word_at` | The cells of the word at a point, given the word characters. `None` when the cell is blank or not a word character. A wide character is never split. |
| `line_at` | The whole logical line through a content line: every soft-wrapped row of it, from the first cell of the first row to the last cell of the last |
| `text` | The text in a range |
| `scroll_by` | Scroll by rows, negative toward the start. Returns the rows actually moved. |
| `pane_request` | For a terminal pane, what its program asked for. `None` for anything else. |

`text` has rules every surface follows:

- Rows are joined with a newline, except a row that soft-wraps into the next,
  which is joined with nothing.
- Trailing blanks of each logical line are dropped.
- A wide character is copied whole when either of its cells is in the range.
- A range taller than the viewport is copied in full, and the surface's
  scroll position is the same after the call as before it.
- There is no trailing newline.

A function in the crate's tests runs these rules against any surface, and the
two built-in surfaces both pass it. An app that writes its own surface can
call it from its tests.

### The surface for app-held text

`LineSurface` borrows the rows the app rendered, one string per visual row,
and the app's scroll position.

- Content line `n` is row `n`.
- Columns are measured with ratatui's `CellWidth` per grapheme cluster, the
  same measure the buffer uses, so a column on screen is the same column in
  the text.
- An optional slice says which rows soft-wrap into the next one.
- `scroll_by` moves the scroll position within the rows.

An app whose view derives its scroll from a cursor (as dif's document view
does) implements the trait itself, using the functions in `words`.

### The surface for a terminal emulator

Behind the `vt100` feature, `Vt100Surface` borrows a `vt100::Screen`.

- **Line 0 is the oldest row of scrollback the emulator holds**, and the live
  screen follows it. With this numbering a selection stays on its text when
  the pane scrolls through history and also when new output arrives, until
  the scrollback is full. Once it is full, each new row of output moves the
  text one line under the selection. That limit is documented, and clearing
  the selection when output arrives is the app's choice.
- On the alternate screen there is no scrollback, and lines are the screen's
  rows.
- Words and lines are found on the emulator's cells: a wide character's
  second cell takes the class of its first, and a line follows the
  emulator's own wrapped flag across rows.
- `text` steps the emulator's scrollback offset to reach rows out of view and
  restores it.
- `pane_request` reads the emulator's mouse tracking mode, its mouse
  encoding, whether the alternate screen is on, and whether application
  cursor keys are on.

### Highlight

A selection is drawn as **at most three rectangles**: the part of its first
row from the start to the right edge, the full rows between, and the part of
its last row from the left edge to the end. A selection on one row is one
rectangle. Rectangles are clipped to the area, so a selection scrolled half
out of view draws only what is visible.

- `HighlightRects` is computed from a range, the surface's origin and the
  area. It is a pure function, with no per-cell test.
- Rendering patches the style of the cells in the rectangles. The style is
  the caller's. The crate's default is reversed video, which reads on dark
  and light backgrounds and with color off. In the toolkit, the theme's
  `Selected` role supplies it.
- Ends are snapped to whole characters, so half of a wide character is never
  highlighted.

## Pass-through into panes

A pane is a surface whose `pane_request` is not `None`. The request says what
the program inside asked its terminal for:

| Field | Values |
| --- | --- |
| Tracking | Off, press only, press and release, button motion, any motion |
| Encoding | Default (three bytes), UTF-8, SGR |
| Alternate screen | On or off |
| Application cursor keys | On or off |

### Routing

A press on a pane:

| Condition, first match wins | Route |
| --- | --- |
| Select mode is on for this pane | Selection |
| The selection modifier is held | Selection |
| Pass-through is off in the settings | Selection |
| Tracking is not off | Pass-through gesture |
| Otherwise | Selection |

The wheel over a pane, with no gesture active:

| Condition, first match wins | Route |
| --- | --- |
| Select mode is on for this pane, or the selection modifier is held, or pass-through is off | Reported to the app, which scrolls the pane's history |
| Tracking is not off | Forwarded as a wheel event |
| The alternate screen is on, and wheel-to-arrows is on in the settings | Forwarded as arrow keys |
| Otherwise | Reported to the app |

Both tables are pure functions of the request, the modifiers and the
settings.

### Encoding

Coordinates are relative to the pane's area and start at 1. The button code
is built the way xterm defines it:

| Part | Value |
| --- | --- |
| Left, middle, right | 0, 1, 2 |
| Release, in the default and UTF-8 encodings | 3 |
| Wheel up, down, left, right | 64, 65, 66, 67 |
| Shift, Alt, Control held | add 4, 8, 16 |
| Motion with a button held | add 32 |

| Encoding | Bytes |
| --- | --- |
| SGR | `CSI <` code `;` column `;` row, then `M` for a press or motion and `m` for a release. The release keeps its button's code. |
| Default | `CSI M`, then three bytes: code + 32, column + 32, row + 32. An event whose column or row is above 223 cannot be written and is dropped. |
| UTF-8 | As default, with each of the three values written as a UTF-8 character. An event above 2015 is dropped. |

What is forwarded depends on tracking:

| Tracking | Press | Release | Drag | Wheel |
| --- | --- | --- | --- | --- |
| Press only | Yes | No | No | No |
| Press and release | Yes | Yes | No | Yes |
| Button motion | Yes | Yes | Yes | Yes |
| Any motion | Yes | Yes | Yes | Yes |

A program that asked for any motion gets button motion: the host terminal is
never asked for hover, so there are no bare moves to forward. This is
documented as the one thing a pane's program cannot get.

### The wheel as arrow keys

A full-screen program that did not ask for the mouse (a pager, an editor with
the mouse off) gets one arrow key per row of the wheel step: `CSI A` and
`CSI B`, or `SS3 A` and `SS3 B` when it turned application cursor keys on.
That is what iTerm2 and xterm.js do for such programs themselves.

### Selecting in a pane whose program has the mouse

The decision is that holding Shift selects instead of passing through. Three
routes reach selection, and only the last works everywhere:

| Route | Where it works |
| --- | --- |
| Shift held at the press | iTerm2 and xterm.js on macOS, where Shift and a click reach the app. Not xterm.js on Linux or Windows: there xterm.js keeps Shift for its own selection and the app never sees the event. |
| The host terminal's own selection | Wherever the terminal offers one while an app has the mouse (Shift in xterm.js on Linux and Windows). It selects screen cells across the whole app, borders included, cannot scroll a pane, and copies through the terminal. |
| **Select mode** | Everywhere, with or without a mouse. See below. |

The app's platform says nothing about the terminal's: a Linux app over SSH
may be drawn by xterm.js on macOS. So the crate does not change behavior by
platform. It makes select mode a required part of every app that embeds a
pane, and the mouse then selects in that pane with no modifier at all. A
second modifier is proposed under [Open questions](#open-questions).

## Select mode and keyboard equivalents

### Select mode

Select mode is selection driven from the keyboard, on one target at a time.
The app starts it from a key or a palette command of its own.

- A cursor appears on the surface: at the point the app passes, or else at
  the start of the last visible row.
- While it is on, the mouse on that target always selects and is never
  passed through, and every key press goes to select mode. A key it does not
  bind is swallowed, so nothing leaks into the pane's program.
- Moving the cursor past the top or bottom of the viewport scrolls the
  surface.
- Copying returns the text to the app and ends the mode. Cancelling clears a
  selection first and ends the mode on the second press.
- A press on another target ends the mode.

The default keys need nothing a first-class configuration lacks: no kitty
keyboard protocol, no Alt.

| Key | Command |
| --- | --- |
| `h` `j` `k` `l`, arrows | Move by one cell or row |
| `w`, `b` | Next word, previous word |
| `0` or Home, `$` or End | Start and end of the line |
| Ctrl+U, Ctrl+D | Half a page up, half a page down |
| PageUp, PageDown | A page up, a page down |
| `g`, `G` | First line, last line |
| `v` or Space | Start or stop selecting by cell from the cursor |
| `V` | Start or stop selecting by line |
| `W` | Select the word under the cursor |
| `y` or Enter | Copy and leave |
| Esc or `q` | Clear the selection, or leave when there is none |

The key map is a public pure function. An app with its own keys turns the
built-in map off in the settings, maps keys to `SelectionCommand` itself and
calls `apply_command`.

### Keyboard equivalents

Every mouse action, and the key path that does the same. "Crate" means this
crate implements it. "App" means the app must bind it, and the example
program shows how.

| Mouse action | Keyboard equivalent | Provided by |
| --- | --- | --- |
| Click a target to focus it | The app's focus keys | App |
| Click a button, tab or row | The command it runs, from the palette or its shortcut | App |
| Wheel over a pane or view | The app's scroll keys: by row, by half page, to the top and bottom | App |
| Drag to select | Select mode: `v`, then motions | Crate |
| Double click a word | Select mode: `W` | Crate |
| Triple click a line | Select mode: `V` | Crate |
| Drag past an edge to scroll while selecting | Select mode: motions scroll the surface | Crate |
| Wheel while selecting | Select mode: Ctrl+U, Ctrl+D, PageUp, PageDown | Crate |
| Shift and click to extend | Select mode: more motions | Crate |
| Release to copy | Select mode: `y` or Enter | Crate, through the same `Copy` outcome |
| Click elsewhere to clear | Select mode: Esc | Crate |
| Shift and drag in a pane whose program has the mouse | Select mode | Crate |
| Click, drag or wheel passed into a pane's program | The program's own keys, typed into the pane | The program |
| Wheel as arrow keys for a full-screen program | The arrow keys | The program |
| Drag a divider | The app's resize commands, if it has dividers | App |

An app built on this crate is not done until it binds the rows marked "App".
In stock tmux those rows and select mode are the whole interface.

## What the crate returns, and what stays the app's decision

Every call returns a `MouseReport<T>`: one outcome, the target a button went
down on if one did, and whether a redraw is due.

| Outcome | Meaning | What the app usually does |
| --- | --- | --- |
| `Ignored` | Not for the crate | Handles the event as if the crate were not there |
| `Consumed` | Handled, with nothing to act on | Nothing, or redraws if due |
| `Pressed` | A button went down on a target, in a plain or selection gesture | Moves focus |
| `Clicked` | Press and release with no drag between | Runs what the target does. The click count and modifiers are included. |
| `Dragged` | A plain gesture's pointer moved | Whatever dragging that target means |
| `DragEnded` | A plain gesture that dragged was released | Finishes it |
| `Wheel` | The wheel turned over a target and the crate did not use it | Scrolls that target, or moves its cursor |
| `Copy` | Text is ready to be copied | Sends it to its clipboard route, and may show a notice |
| `Forward` | Bytes for a pane's program | Writes them to that pane |

What the crate never decides:

- **Focus.** It reports the pressed target. Whether a click moves focus is
  the app's.
- **What a click does.** Opening a link, switching a tab, running a command.
- **Scrolling outside a selection.** The app scrolls its panes and views, or
  moves a cursor instead, as dif's document view does.
- **The clipboard**, and what to tell the user after a copy.
- **Where forwarded bytes go.** The app writes them to the pane.
- **When the mouse is off.** While a modal is open the app either skips the
  crate or pushes a backdrop.
- **When a selection is stale.** The crate clears a selection on a new press,
  on a key press or a resize when the settings say so, and when told. Clearing
  it when a view changes or when a pane prints is the app's call:
  `clear_selection` and `forget_target` exist for that.
- **Key bindings** outside select mode.
- **The highlight's style.**

### The clipboard seam

The crate has no clipboard. `Copy { target, text }` is the seam: the app
passes `text` to whatever route it uses. In the toolkit that is
`avandar_tui_term::clipboard`, which picks between a local tool and OSC 52.
With copy on release off, nothing is returned at release, the selection
stays, and the app calls `selected_text` when its own copy command runs.

## Settings

`MouseSettings` holds every tunable value. The defaults are dif's values
where dif has one.

| Setting | Default | Meaning |
| --- | --- | --- |
| `wheel_step_rows` | 3 | Rows per notch of the wheel |
| `multi_click_interval` | 500 ms | The longest gap between presses of one multi-click |
| `multi_click_distance` | 1 | How many cells apart two presses of one multi-click may be |
| `word_characters` | Letters, digits and `_-./~:@+%#?&=` | What a word is made of. The default takes a path, a URL or `file.rs:12` whole. |
| `should_copy_on_release` | true | Return the text for copying when a selection is finished |
| `should_keep_selection_after_copy` | true | Leave the copied selection highlighted |
| `should_clear_on_key_press` | true | A key press outside select mode clears the selection. A bare modifier never does. |
| `should_clear_on_resize` | true | A resize clears the selection |
| `selection_modifiers` | Shift | Held at a press on a pane, they select instead of passing through. Any one of them is enough. |
| `is_passthrough_enabled` | true | Forward the mouse to panes whose programs asked for it |
| `is_wheel_to_arrows_enabled` | true | Send arrow keys for the wheel to full-screen programs that did not ask for the mouse |
| `should_handle_select_mode_keys` | true | Use the built-in select mode key map |
| `auto_scroll_interval` | 50 ms | Time between edge-scroll steps |
| `auto_scroll_max_rows` | 10 | The most rows one edge-scroll step moves |
| `edge_margin_rows` | 0 | Rows at the top and bottom of an area that count as past the edge |
| `redraw_interval` | 16 ms | The shortest time between two redraws the crate asks for |

How scrolling feels (the interval, the maximum, the wheel step) is on the
release checklist of judgments, and the defaults may move before the first
tag.

## Public API

Signatures and doc comments only. `T` is the app's target type. Every item
is re-exported from the crate root.

### Modes

```rust
/// The bytes that turn mouse reporting on: press and release (1000), drags
/// while a button is held (1002) and SGR coordinates (1006), in that order.
/// Hover reporting (1003) is not among them and never will be.
pub const ENABLE_MOUSE_REPORTING: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

/// The bytes that turn every mouse tracking mode and coordinate format off.
/// Write these from a panic hook.
pub const DISABLE_MOUSE_REPORTING: &[u8] =
    b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";

/// Turns mouse reporting on.
///
/// On macOS and Linux this writes [`ENABLE_MOUSE_REPORTING`] to `writer` and
/// flushes. On Windows it turns mouse input on in the console's input mode
/// and writes nothing.
///
/// # Errors
///
/// [`MouseError::EnableReporting`] when the write or the console call fails.
pub fn enable_mouse_reporting(writer: &mut impl Write) -> Result<(), MouseError>;

/// Turns mouse reporting off. Safe to call when it was never on, and safe to
/// call twice.
///
/// # Errors
///
/// [`MouseError::DisableReporting`] when the write or the console call fails.
pub fn disable_mouse_reporting(writer: &mut impl Write) -> Result<(), MouseError>;

/// Mouse reporting that is on for as long as this value lives.
///
/// Dropping it turns reporting off and ignores a failure, since a drop
/// cannot report one. Call [`MouseReporting::disable`] to see the error.
pub struct MouseReporting<W: Write> { /* private */ }

impl<W: Write> MouseReporting<W> {
    /// Turns reporting on and returns the guard that turns it off.
    ///
    /// `writer` is the stream the app draws on.
    ///
    /// # Errors
    ///
    /// [`MouseError::EnableReporting`] when reporting could not be turned on.
    pub fn enable(writer: W) -> Result<Self, MouseError>;

    /// Turns reporting off now and reports a failure.
    ///
    /// # Errors
    ///
    /// [`MouseError::DisableReporting`] when it could not be turned off.
    pub fn disable(self) -> Result<(), MouseError>;
}
```

### Hit areas

```rust
/// The interactive areas of one frame, each with the app's own target.
///
/// Clear it at the start of a draw and push an area for everything that
/// reacts to the mouse. Areas pushed later are on top.
pub struct HitAreas<T> { /* private */ }

impl<T> HitAreas<T> {
    /// An empty set of areas.
    pub const fn new() -> Self;

    /// Removes every area, keeping the allocation for the next frame.
    pub fn clear(&mut self);

    /// Adds `area` on top of the ones already pushed. An empty area is
    /// ignored.
    pub fn push(&mut self, area: Rect, target: T);

    /// The topmost target whose area contains `position`.
    pub fn target_at(&self, position: Position) -> Option<&T>;

    /// The topmost target whose area contains `position`, with its area.
    pub fn hit_at(&self, position: Position) -> Option<(Rect, &T)>;

    /// The area most recently pushed for `target`.
    pub fn area_of(&self, target: &T) -> Option<Rect>
    where
        T: PartialEq;

    /// The areas from the bottom up.
    pub fn iter(&self) -> impl Iterator<Item = (Rect, &T)>;

    /// Whether no area has been pushed.
    pub fn is_empty(&self) -> bool;
}

/// A pointer position moved into an area.
pub struct ClampedPosition {
    /// The nearest cell inside the area.
    pub position: Position,
    /// How many rows the pointer was outside: negative above the area,
    /// positive below it, zero inside.
    pub overshoot_rows: i32,
}

/// Clamps `position` into `area`, treating the outermost `edge_margin_rows`
/// rows as outside. `None` when the area is empty.
pub fn clamp_position_into(
    area: Rect,
    position: Position,
    edge_margin_rows: u16,
) -> Option<ClampedPosition>;
```

### Clicks

```rust
/// Counts presses that follow each other closely enough to be one
/// double or triple click.
pub struct ClickCounter<T> { /* private */ }

impl<T: Clone + PartialEq> ClickCounter<T> {
    /// A counter that has seen no press.
    pub const fn new() -> Self;

    /// Records a left press and returns its place in the run: 1, 2 or 3.
    /// The press after a third starts again at 1.
    pub fn register_press(
        &mut self,
        target: &T,
        position: Position,
        now: Instant,
        settings: &MouseSettings,
    ) -> u8;

    /// Forgets the last press, so the next one counts as the first.
    pub fn reset(&mut self);
}
```

### Content coordinates

```rust
/// A cell in a surface's content, which stays on its text while the surface
/// scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentPoint {
    /// The content line, numbered by the surface.
    pub line: i64,
    /// The column in cells from the start of the content row.
    pub column: u32,
}

impl ContentPoint {
    /// The content point under `position`, which must be inside `area`.
    /// `origin` is the surface's origin.
    pub fn from_screen_position(position: Position, area: Rect, origin: Self) -> Self;

    /// Where this point is on screen, or `None` when it is outside `area`.
    pub fn to_screen_position(self, area: Rect, origin: Self) -> Option<Position>;
}

/// A run of content cells in reading order. Both ends are included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentRange {
    /// The first cell.
    pub start: ContentPoint,
    /// The last cell.
    pub end: ContentPoint,
}

impl ContentRange {
    /// The range between two points, given in either order.
    pub fn new(first: ContentPoint, second: ContentPoint) -> Self;

    /// Whether `point` is inside the range.
    pub fn contains(self, point: ContentPoint) -> bool;

    /// The smallest range that covers both.
    pub fn union(self, other: Self) -> Self;
}
```

### Surfaces

```rust
/// Something text can be selected in. The app implements it once per kind
/// of surface, or uses [`LineSurface`] and `Vt100Surface`.
pub trait SelectableSurface {
    /// The content point shown in the top-left cell of the surface's area.
    fn origin(&self) -> ContentPoint;

    /// The first and last content line that exist.
    fn line_bounds(&self) -> RangeInclusive<i64>;

    /// The cells of the character at `point`: one cell, or both cells of a
    /// wide character from either half.
    fn character_at(&mut self, point: ContentPoint) -> ContentRange;

    /// The cells of the word at `point`, or `None` when the cell is blank or
    /// holds no word character.
    fn word_at(
        &mut self,
        point: ContentPoint,
        word_characters: &WordCharacters,
    ) -> Option<ContentRange>;

    /// The whole logical line through `line`, across its soft-wrapped rows.
    fn line_at(&mut self, line: i64) -> ContentRange;

    /// The text in `range`: soft-wrapped rows joined with nothing, other rows
    /// with a newline, trailing blanks dropped, wide characters whole, no
    /// trailing newline. The scroll position is unchanged afterwards.
    fn text(&mut self, range: ContentRange) -> String;

    /// Scrolls by `rows`, negative toward the start, and returns the rows
    /// actually moved.
    fn scroll_by(&mut self, rows: i32) -> i32;

    /// For a terminal pane, what the program inside asked for. `None` for
    /// any other surface.
    fn pane_request(&self) -> Option<PaneMouseRequest> {
        None
    }
}

/// The app's surfaces, looked up by target.
pub trait SurfaceSet<T> {
    /// The surface behind `target`, or `None` when it has no selectable
    /// text.
    fn surface_mut(&mut self, target: &T) -> Option<&mut dyn SelectableSurface>;
}

/// The surface set of an app with nothing to select.
pub struct NoSurfaces;

impl<T> SurfaceSet<T> for NoSurfaces { /* always `None` */ }

/// Where an app-held view is scrolled to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollPosition {
    /// The row shown at the top.
    pub line: usize,
    /// The column shown at the left edge, in cells.
    pub column: u32,
}

/// A surface over rows of text the app rendered itself.
pub struct LineSurface<'a, L: AsRef<str>> { /* private */ }

impl<'a, L: AsRef<str>> LineSurface<'a, L> {
    /// A surface over `rows`, one string per visual row, scrolled to
    /// `scroll`, showing `visible_rows` rows.
    pub fn new(rows: &'a [L], scroll: &'a mut ScrollPosition, visible_rows: u16) -> Self;

    /// Says which rows soft-wrap into the next one. `is_row_wrapped` has one
    /// entry per row.
    #[must_use]
    pub fn with_wrapped_rows(self, is_row_wrapped: &'a [bool]) -> Self;
}

impl<L: AsRef<str>> SelectableSurface for LineSurface<'_, L> { /* ... */ }
```

### Words

```rust
/// The first and last column, in cells, of the word in `line` at `column`.
/// `None` when that cell is past the end, blank, or not a word character.
pub fn word_columns_in_line(
    line: &str,
    column: u32,
    word_characters: &WordCharacters,
) -> Option<(u32, u32)>;

/// The first and last column, in cells, of the character in `line` at
/// `column`. `None` past the end of the line.
pub fn character_columns_in_line(line: &str, column: u32) -> Option<(u32, u32)>;

/// The part of `line` between two columns, both included, with every
/// character that has a cell in the range kept whole.
pub fn text_in_columns(line: &str, first_column: u32, last_column: u32) -> &str;
```

### Selection and highlight

```rust
/// What a selection grows by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionUnit {
    /// Cells: a press and a drag.
    Cell,
    /// Whole words: a double click.
    Word,
    /// Whole lines: a triple click.
    Line,
}

/// How far along a selection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStage {
    /// The button is down and the pointer has not moved.
    Pressed,
    /// The pointer has moved with the button down.
    Dragging,
    /// The button came up. The selection stays until it is cleared.
    Done,
}

/// Selected text on one target. A selection never spans targets.
pub struct Selection<T> { /* private */ }

impl<T> Selection<T> {
    /// The target the selection is on.
    pub const fn target(&self) -> &T;

    /// The selected cells.
    pub const fn range(&self) -> ContentRange;

    /// What the selection grows by.
    pub const fn unit(&self) -> SelectionUnit;

    /// How far along the selection is.
    pub const fn stage(&self) -> SelectionStage;
}

/// The style [`HighlightRects::render`] uses when the app has no theme:
/// reversed video.
pub const DEFAULT_HIGHLIGHT_STYLE: Style;

/// A selection as at most three screen rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightRects { /* private */ }

impl HighlightRects {
    /// The rectangles that cover `range` inside `area`, for a surface whose
    /// origin is `origin`. Empty when none of the range is visible.
    pub fn from_range(range: ContentRange, origin: ContentPoint, area: Rect) -> Self;

    /// The rectangles, from top to bottom.
    pub fn as_slice(&self) -> &[Rect];

    /// Patches `style` onto every cell in the rectangles.
    pub fn render(&self, buffer: &mut Buffer, style: Style);
}
```

### Pass-through

```rust
/// Which mouse events a pane's program asked its terminal to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseTracking {
    /// None.
    Off,
    /// Presses only (mode 9).
    Press,
    /// Presses and releases (mode 1000).
    PressRelease,
    /// Presses, releases, and moves with a button held (mode 1002).
    ButtonMotion,
    /// Every move (mode 1003). Served as [`MouseTracking::ButtonMotion`].
    AnyMotion,
}

/// How a pane's program asked for mouse events to be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseEncoding {
    /// Three bytes after `CSI M`.
    Default,
    /// Three UTF-8 characters after `CSI M` (mode 1005).
    Utf8,
    /// Numbers after `CSI <` (mode 1006).
    Sgr,
}

/// What the program in a pane asked its terminal for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneMouseRequest {
    /// Which events it wants.
    pub tracking: MouseTracking,
    /// How it wants them written.
    pub encoding: MouseEncoding,
    /// Whether it is on the alternate screen.
    pub is_alternate_screen: bool,
    /// Whether it turned application cursor keys on.
    pub is_application_cursor: bool,
}

/// Where a press on a pane goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressRoute {
    /// It starts a selection.
    Select,
    /// It is forwarded to the program.
    Forward,
}

/// Where the wheel over a pane goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelRoute {
    /// It is reported to the app.
    Report,
    /// It is forwarded to the program as a wheel event.
    Forward,
    /// It is forwarded to the program as arrow keys.
    ArrowKeys,
}

/// Decides where a press on a pane goes.
pub fn press_route(
    request: &PaneMouseRequest,
    modifiers: KeyModifiers,
    is_select_mode_on: bool,
    settings: &MouseSettings,
) -> PressRoute;

/// Decides where the wheel over a pane goes.
pub fn wheel_route(
    request: &PaneMouseRequest,
    modifiers: KeyModifiers,
    is_select_mode_on: bool,
    settings: &MouseSettings,
) -> WheelRoute;

/// The bytes that report `event` to a pane's program in the encoding it
/// asked for, with coordinates relative to `area`. `None` when the program
/// did not ask for this kind of event, or the encoding cannot hold the
/// coordinates.
pub fn build_mouse_report(
    event: &MouseEvent,
    area: Rect,
    request: &PaneMouseRequest,
) -> Option<Vec<u8>>;

/// The arrow keys that stand for `rows` of wheel: up when negative, down
/// when positive, in the form application cursor keys calls for.
pub fn build_wheel_arrow_keys(rows: i32, is_application_cursor: bool) -> Vec<u8>;
```

### Settings

```rust
/// The characters, besides letters and digits, that are part of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordCharacters { /* private */ }

impl WordCharacters {
    /// Letters, digits and each character of `punctuation`.
    pub fn new(punctuation: impl Into<String>) -> Self;

    /// Whether `character` is part of a word.
    pub fn contains(&self, character: char) -> bool;
}

impl Default for WordCharacters { /* `_-./~:@+%#?&=` */ }

/// Every tunable value. See the settings table for meanings and defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MouseSettings {
    pub wheel_step_rows: u16,
    pub multi_click_interval: Duration,
    pub multi_click_distance: u16,
    pub word_characters: WordCharacters,
    pub should_copy_on_release: bool,
    pub should_keep_selection_after_copy: bool,
    pub should_clear_on_key_press: bool,
    pub should_clear_on_resize: bool,
    pub selection_modifiers: KeyModifiers,
    pub is_passthrough_enabled: bool,
    pub is_wheel_to_arrows_enabled: bool,
    pub should_handle_select_mode_keys: bool,
    pub auto_scroll_interval: Duration,
    pub auto_scroll_max_rows: u16,
    pub edge_margin_rows: u16,
    pub redraw_interval: Duration,
}

impl Default for MouseSettings { /* the defaults in the settings table */ }
```

Each field carries a doc comment that repeats its row of the settings table.

### Outcomes

```rust
/// What one call to the controller produced.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct MouseReport<T> {
    /// What happened.
    pub outcome: MouseOutcome<T>,
    /// The target a button went down on in this event, whatever else the
    /// press did. Most apps move focus to it.
    pub pressed_target: Option<T>,
    /// Whether the app should draw a frame now.
    pub is_redraw_due: bool,
}

/// A button press on a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerPress {
    /// The button.
    pub button: MouseButton,
    /// Where on screen.
    pub position: Position,
    /// Where in the target's content, when it has a surface.
    pub content_point: Option<ContentPoint>,
    /// 1, 2 or 3 for a single, double or triple click.
    pub clicks: u8,
    /// The modifiers held.
    pub modifiers: KeyModifiers,
}

/// What happened, for the app to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MouseOutcome<T> {
    /// The event is not the crate's. Handle it as usual.
    Ignored,
    /// The crate handled the event and there is nothing to act on.
    Consumed,
    /// A button went down on `target`.
    Pressed { target: T, press: PointerPress },
    /// A button went down and came up on `target` with no drag between.
    Clicked { target: T, press: PointerPress },
    /// The pointer moved during a plain gesture owned by `target`.
    Dragged { target: T, button: MouseButton, from: Position, to: Position },
    /// A plain gesture that dragged was released.
    DragEnded { target: T, button: MouseButton, from: Position, to: Position },
    /// The wheel turned over `target` and the crate did not use it. `rows`
    /// and `columns` are negative toward the start.
    Wheel { target: T, rows: i32, columns: i32, modifiers: KeyModifiers },
    /// `text` was selected on `target` and should go to the clipboard.
    Copy { target: T, text: String },
    /// `bytes` should be written to the program in the pane `target`.
    Forward { target: T, bytes: Vec<u8> },
}
```

### The controller

```rust
/// The mouse state of one app: the gesture in progress, the selection, the
/// click count and select mode. The app owns one and passes it every event.
pub struct MouseController<T> { /* private */ }

impl<T: Clone + PartialEq> MouseController<T> {
    /// A controller with no gesture and no selection.
    pub fn new(settings: MouseSettings) -> Self;

    /// The settings in use.
    pub const fn settings(&self) -> &MouseSettings;

    /// The settings, to change them between events.
    pub const fn settings_mut(&mut self) -> &mut MouseSettings;

    /// Handles one terminal event.
    ///
    /// Pass every event, before the app's own handling: mouse events drive
    /// gestures, key presses drive select mode and clear a selection, and a
    /// resize or a lost focus cancels a gesture. `areas` are those of the
    /// last frame drawn. Key releases are ignored.
    pub fn handle_event<S: SurfaceSet<T>>(
        &mut self,
        event: &Event,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T>;

    /// Does what is due at `now`: an edge-scroll step, a held-back redraw.
    /// Call it on every pass of the loop. It does nothing when nothing is
    /// due.
    pub fn tick<S: SurfaceSet<T>>(
        &mut self,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T>;

    /// The moment [`MouseController::tick`] next has work, if any. Wait for
    /// input no longer than this.
    pub fn next_deadline(&self) -> Option<Instant>;

    /// The selection to highlight, if there is one.
    pub fn selection(&self) -> Option<&Selection<T>>;

    /// The text of the selection, if there is one.
    pub fn selected_text<S: SurfaceSet<T>>(&self, surfaces: &mut S) -> Option<String>;

    /// Drops the selection. Returns whether there was one.
    pub fn clear_selection(&mut self) -> bool;

    /// The target that owns the gesture in progress.
    pub fn gesture_owner(&self) -> Option<&T>;

    /// Ends the gesture in progress with no click and no copy. Returns
    /// whether there was one.
    pub fn cancel_gesture(&mut self) -> bool;

    /// Drops everything held about `target`: its gesture, its selection, its
    /// select mode. Call it when the target goes away. Returns whether
    /// anything was dropped.
    pub fn forget_target(&mut self, target: &T) -> bool;

    /// Starts select mode on `target`, with the cursor at `start` or, when
    /// `None`, at the start of the last visible row. Ignored when the target
    /// has no surface or no area.
    pub fn enter_select_mode<S: SurfaceSet<T>>(
        &mut self,
        target: T,
        start: Option<ContentPoint>,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T>;

    /// Ends select mode and drops its selection. Returns whether it was on.
    pub fn exit_select_mode(&mut self) -> bool;

    /// The target select mode is on for.
    pub fn select_mode_target(&self) -> Option<&T>;

    /// Where the select mode cursor is in its target's content. The app
    /// draws it, usually by placing the terminal's cursor there.
    pub fn select_mode_cursor(&self) -> Option<ContentPoint>;

    /// Runs one select mode command. Ignored when select mode is off.
    pub fn apply_command<S: SurfaceSet<T>>(
        &mut self,
        command: SelectionCommand,
        now: Instant,
        areas: &HitAreas<T>,
        surfaces: &mut S,
    ) -> MouseReport<T>;
}
```

### Select mode

```rust
/// A cursor movement in select mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionMotion {
    Left, Right, Up, Down,
    WordForward, WordBackward,
    LineStart, LineEnd,
    HalfPageUp, HalfPageDown, PageUp, PageDown,
    FirstLine, LastLine,
}

/// What select mode can be told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionCommand {
    /// Move the cursor, extending the selection when one is being made.
    Move(SelectionMotion),
    /// Start selecting by cell at the cursor, or stop.
    ToggleCellSelection,
    /// Start selecting by line at the cursor, or stop.
    ToggleLineSelection,
    /// Select the word under the cursor.
    SelectWord,
    /// Return the selected text for copying and leave select mode.
    Copy,
    /// Clear the selection, or leave select mode when there is none.
    Cancel,
}

/// The built-in select mode key map. `None` for a key it does not bind and
/// for a key release.
pub fn selection_command_from_key(key: &KeyEvent) -> Option<SelectionCommand>;
```

Each variant carries its own doc comment in the source.

### The `vt100` feature

```rust
/// A surface over a terminal emulator's screen and its scrollback.
pub struct Vt100Surface<'a> { /* private */ }

impl<'a> Vt100Surface<'a> {
    /// A surface over `screen`. Line 0 is the oldest row of scrollback the
    /// emulator holds.
    pub fn new(screen: &'a mut vt100::Screen) -> Self;
}

impl SelectableSurface for Vt100Surface<'_> { /* ... */ }

impl PaneMouseRequest {
    /// What the program writing to `screen` asked for.
    pub fn from_screen(screen: &vt100::Screen) -> Self;
}
```

### Errors

```rust
/// What can go wrong in this crate.
#[derive(Debug)]
#[non_exhaustive]
pub enum MouseError {
    /// Mouse reporting could not be turned on.
    EnableReporting(std::io::Error),
    /// Mouse reporting could not be turned off.
    DisableReporting(std::io::Error),
}

impl std::fmt::Display for MouseError { /* one sentence per variant */ }
impl std::error::Error for MouseError { /* `source` is the I/O error */ }
```

Nothing else in the crate fails: a gesture on a target that has gone away is
cancelled, and an event that cannot be encoded is dropped.

### How an app calls it

Not part of the API: the shape of the loop the API assumes.

```rust
let mut controller = MouseController::new(MouseSettings::default());
let mut areas = HitAreas::new();
loop {
    let now = Instant::now();
    let wait = controller
        .next_deadline()
        .map_or(idle_wait, |deadline| deadline.saturating_duration_since(now));
    let mut report = None;
    if event::poll(wait)? {
        let event = event::read()?;
        let mut surfaces = app.surfaces();
        let event_report =
            controller.handle_event(&event, Instant::now(), &areas, &mut surfaces);
        if matches!(event_report.outcome, MouseOutcome::Ignored) {
            app.handle_event(&event);
        }
        report = Some(event_report);
    }
    let tick_report = controller.tick(Instant::now(), &areas, &mut app.surfaces());
    // The app acts on each report's outcome and pressed target, then draws
    // when either report says a redraw is due. Drawing clears `areas` and
    // pushes this frame's areas.
}
```

## Test plan

The layers are the ones in [`../testing.md`](../testing.md). The crate reads
and writes the terminal, so all seven apply.

### The example program: `mouse_lab`

`crates/ratatui-mouse/examples/mouse_lab.rs`, built with the `vt100` feature.
It is the program every harness drives, and it follows the rules for example
programs in `architecture.md`.

- **Left pane:** app-held text through `LineSurface`. Its fixed content has
  ASCII words, a path with a line number, a CJK word, a line that soft-wraps
  over three rows, and more rows than fit.
- **Right pane:** a `vt100` parser filled from a fixed script, with no child
  process: history longer than the pane, a wrapped line and a wide
  character. Keys switch what the parser was told, so pass-through is tested
  without a real program: `m` cycles the mouse request (off, press and
  release with SGR, button motion with SGR, press and release with the
  default encoding) and `a` toggles the alternate screen. Forwarded bytes
  are logged, not written anywhere.
- **A tab strip and a divider**, as plain targets.
- **Keys:** Tab moves focus, `s` starts select mode on the focused pane,
  PageUp and PageDown scroll it, `q` quits, `!` panics.
- **Flags:** `--log <path>`, `--query-modes` (after setup and after
  teardown, ask the terminal for modes 9, 1000, 1002, 1003, 1005, 1006, 1015
  and 1016 and log the answers), `--settings <json>` to override defaults.
- **The log** has one line per: sequence written, event received, report
  returned (outcome, pressed target, redraw flag), deadline scheduled, tick
  that did work, and the selection after each change (target, range, unit,
  stage). The harness asserts on those lines and on the terminal's own state,
  never on pixels.

### Layer 1: unit and escape-sequence tests

Pure functions, tested by calling them, on all three platforms.

| Area | Proves |
| --- | --- |
| Modes | The setup bytes are exactly M1. They do not contain `1003`. They contain no reset. The teardown bytes are exactly M5. The guard writes setup once and teardown once, on drop and on `disable`. On Windows no sequence is written. |
| Hit areas | The topmost area wins. Borders and gaps hit nothing. An empty area is never hit. Clamping gives the nearest cell and the signed overshoot, with and without a margin. |
| Clicks | 1, 2, 3, 1. A press one cell away continues the run and two cells away restarts it. A slow press, another target or a drag restarts it. |
| Gestures | The owner gets drags and the release from outside its area. A second press cancels the first gesture with no click. A vanished owner cancels. Each row of the gesture table. |
| Stages | A press and release is a click. A drag that returns to its first cell is a one-character selection. A double click selects on the press. Blank text is dropped. |
| Word and line drags | The original word or line stays selected on both sides. |
| Edge scrolling | The first step is due at once, later ones one interval apart, the step grows with distance and stops at the maximum, the deadline clears at the end of the content, drag events alone never scroll. |
| Wheel | Each row of the wheel table, including the selection's end moving. |
| Redraw budget | At most one due redraw per interval, and a held-back one arrives on the tick at the deadline. |
| Words | Paths, URLs and `file.rs:12` are one word. Brackets and quotes are not word characters. A wide character is whole from either half. Columns are cells. |
| `LineSurface` | The shared surface rules, wrapped rows, wide characters, scrolling limits. |
| `Vt100Surface` | The shared surface rules on a real parser: across scrollback, taller than the pane, the scroll offset restored, line numbers stable while output arrives. |
| Highlight | One, two and three rectangles. Clipping at each edge. Wide characters snapped. |
| Pass-through | Both routing tables, row by row. Every encoding for press, release, drag and wheel with each modifier. Coordinates relative to the pane. Limits of the default and UTF-8 encodings. Arrow keys in both cursor modes. |
| Select mode | Every command. The cursor scrolling the surface. Unbound keys swallowed. The key map, including that a release maps to nothing. |
| Settings | Each setting changes what its row says and nothing else. |

### Layer 2: cross-target lint

The `modes` platform split type-checks and lints for the Windows and Linux
targets.

### Layers 3 and 7: the xterm.js harness, on macOS, Linux and Windows

The harness starts `mouse_lab` in a real xterm.js, waits for the ready line
and for xterm.js to report its mouse mode, sends real pointer and key events,
and reads the log.

| # | Scenario | Asserts |
| --- | --- | --- |
| X1 | Start, then quit | xterm.js reports drag tracking after setup and no tracking after quit. With `--query-modes`, 1002 and 1006 are set, 1003 is reset. |
| X2 | Panic with `!` | xterm.js reports no tracking afterwards |
| X3 | Move the pointer with no button | No event is logged (macOS and Linux) |
| X4 | Click each pane, the tab strip, the divider | `Pressed` then `Clicked` for the right target |
| X5 | Drag across three rows of the left pane | One `Copy` whose text is exactly the expected characters |
| X6 | Double click the path, triple click the wrapped line | `Copy` with the whole path, and with the whole line as one line |
| X7 | Drag from the right pane to below it and hold | Ticks scroll the pane. The harness waits until the logged selection reaches a known history line, then releases. The copied text includes rows that were never on screen together. |
| X8 | Wheel over each pane | `Wheel` with the wheel step, for the pane under the pointer, focused or not |
| X9 | Wheel during a drag | The selection's end moves and no `Wheel` is reported |
| X10 | `m`, then click, drag and wheel on the right pane | `Forward` with the exact bytes for each request, in pane coordinates |
| X11 | `m`, then Shift and drag on the right pane | macOS: a selection and a `Copy`. Linux and Windows: the measured behavior, recorded in `terminals.md` (no event reaches the app, and xterm.js makes its own selection). |
| X12 | `a`, then wheel on the right pane | `Forward` with arrow keys |
| X13 | Keys only: Tab, `s`, motions, `v`, `V`, `W`, `y` | `Copy` with the expected text, and no mouse event in the log |
| X14 | Right click, Control and click | `Pressed` and `Clicked` with the button and modifier |

On Windows (layer 7) the same scenarios run through the pseudo-console on
the hosted runners, the event logs are uploaded, and X1 records which
tracking mode xterm.js ended up in.

### Layer 4: tmux in the harness

| Configuration | Scenarios | Asserts |
| --- | --- | --- |
| Stock tmux | X1, X2, X13 | No mouse event is ever logged, and the keyboard path copies. Setup and teardown still leave tmux's modes as M1 and M5 say. |
| Full tmux | X1 to X14 | The same as outside tmux, through tmux |

### Layer 5: Orca

Orca's command line can type and read but not move a pointer. The capability
probe's mouse section runs in a live Orca terminal and its answers are
compared with `terminals.md`: the one-slot rule, what M1 leaves set, what M5
leaves reset. Then `mouse_lab` runs X1 with `--query-modes` and X13.

### Layer 6: iTerm2 in the virtual machine

Through VNC, as real pointer events: X1 to X10 and X12 to X14, alone and
inside tmux with both configurations. Three checks belong to iTerm2:

- With `--query-modes`, 1002 and 1006 are set and 1003 is reset after setup.
- Moving the pointer with no button logs nothing.
- X11 with Shift gives a selection.

Right click and Control with a click are not reported by iTerm2 with its
default settings, so X14 asserts that nothing arrives and that nothing
breaks.

### What is not automated

Whether edge scrolling and the wheel step feel right, and whether the
highlight reads well, are on the release checklist for a person.

## How dif adopts it

dif (`AvandarLabs/avandar`, under
`agent-skills/public-skills/skills/diff-review/scripts/dif/`) has every piece
of this crate's ancestry in four files, tied to two fixed panes. It is on
ratatui 0.30, crossterm 0.29 and `vt100` 0.16 already, so nothing has to be
upgraded first.

```toml
ratatui-mouse = { git = "https://github.com/AvandarLabs/avandar-tui-kit", tag = "v0.1.0", features = ["vt100"] }
```

| In dif today | Becomes |
| --- | --- |
| `src/tui/mouse.rs`: the mode bytes | `enable_mouse_reporting` at startup. Teardown gains the full M5 sequence, and a panic hook that writes it, which dif lacks today. |
| `src/tui/mouse.rs`: `PaneAreas` with two fixed fields | A `HitAreas<Target>` the draw pass fills. `Target` is dif's own enum: the main pane with the view it is showing, the agent pane, and each tab of the view strip, which makes the tabs clickable. |
| `src/tui/mouse.rs`: `handle_mouse(&mut App, ..)` | One call to `handle_event`, then a match on the report in dif |
| `src/tui/multi_click.rs` | Deleted. The crate counts clicks and finds words and lines. |
| `src/tui/selection.rs` | Deleted. The draw pass calls `HighlightRects::from_range` and `render`. |
| `src/tui/app/select.rs` | Replaced by a small `SurfaceSet<Target>`: a `Vt100Surface` for the agent pane and for the log view, and dif's own surface for the markdown views, built on the `words` functions because those views derive their scroll from a cursor |
| `src/clipboard.rs` | Stays until dif takes `avandar-tui-term`. dif calls it on `Copy`. |

What dif's match on the report does:

| Report | dif |
| --- | --- |
| A pressed target | Sets focus to that pane |
| `Clicked` on a tab | Switches the view |
| `Wheel` | Scrolls the pane, or moves the cursor in a markdown view, as today |
| `Copy` | Copies, and shows "Copied N characters" or the failure |
| `Forward` | Sends the bytes to the pane |
| A due redraw | Nothing new: dif draws every pass |

What stays in dif, unchanged in behavior:

- Ignoring the mouse while a modal is open: dif skips the controller then.
- Hiding a selection when the main view changes. It falls out of the target
  carrying the view: the draw pass highlights a selection only when its
  target is the one on screen.
- The wheel moving the markdown cursor.
- The copy notice.

What changes for a dif user, all of it from the gesture model:

- A double or triple click no longer has to land on the exact same cell.
- Dragging after a double or triple click extends by words or lines.
- A drag that returns to where it started keeps its selection.
- Holding the pointer past a pane's edge keeps scrolling, and the wheel
  works during a drag.
- The mouse reaches a program in the agent pane that asks for it, and the
  wheel sends arrow keys to a full-screen program there.
- A new palette command, "Select text", starts select mode in the focused
  pane, which is how dif becomes usable in stock tmux.

The loop changes in two lines: the poll timeout becomes the smaller of dif's
50 ms and the time to `next_deadline()`, and `tick` is called on each pass.

Order of work in dif: areas and the controller first with pass-through off,
which must leave every existing behavior and test intact. Then the deletions.
Then pass-through on and the select mode command.

## Open questions

1. **Selection where Shift never reaches the app** (xterm.js on Linux and
   Windows). *Decided: select mode covers it and is required. Alt joins
   Shift in the default `selection_modifiers` on every platform once the
   harness on Linux and Windows shows that Alt with a click and Alt with a
   drag reach the app.* xterm.js reserves only Shift there, and on macOS
   Option with a click is already measured to arrive. The cost is that a
   pane's program never receives Alt with the mouse, which few programs use.
   Until that measurement the default stays Shift alone. The decision is
   recorded in `decisions.md`.
2. **Which tracking mode the Windows pseudo-console asks xterm.js for.** The
   crate has to turn mouse input on through the console API, and what the
   pseudo-console then requests is its choice. If it requests every move
   (mode 1003), hover events reach the app and the terminal's hover features
   are affected on Windows. *Recommendation: build it as specified, measure
   it in X1 on the runners, ignore bare moves either way, and record the
   answer in `terminals.md`. If it is 1003, that is documented as the Windows
   behavior: the alternative is no mouse on Windows.*
3. **Wheel direction in iTerm2.** An injected wheel-down arrived as wheel-up,
   which `terminals.md` marks to verify. *Recommendation: the crate reports
   the direction the terminal sends and never flips it.* Natural scrolling is
   the user's system setting, and the terminal has already applied it.
4. **Which modifier gives iTerm2's own selection while an app has the
   mouse.** Not measured. *Recommendation: measure it in the virtual machine
   and put it in the crate's docs for users.* It changes no design.
5. **Depending on `ratatui-core` instead of `ratatui`.** The crate needs only
   core types, and ratatui advises libraries to depend on the core crate.
   *Recommendation: switch before the crate goes to crates.io, not before.*
   Inside the workspace the two give the same types, and one ratatui entry in
   the root manifest is simpler until then.
6. **Clearing a pane's selection when it prints.** The numbering keeps a
   selection on its text until the scrollback is full. *Recommendation: leave
   it to the app, and have `avandar-tui-pty` report when output arrived so an
   app can clear at the cap if it wants.*
