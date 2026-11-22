# Mouse reporting

`ratatui-mouse` provides reporting setup, frame hit areas, content coordinates,
click counting, settings, borrowed text surfaces, selection reports, highlight
geometry, and pane routing and encoders for ratatui apps using crossterm. It is
a library building block: the app owns its event loop, drawing stream, terminal
lifecycle, and panic hook. Gesture behavior follows the
[mouse specification](specs/ratatui-mouse.md).

## Using reporting

Pass the stream the app draws on to `enable_mouse_reporting` and
`disable_mouse_reporting`, or give it to `MouseReporting::enable`. The guard
also accepts a mutable reference to that stream. Dropping an active guard
attempts teardown once and ignores any I/O error. Calling its consuming
`disable` method returns cleanup errors; successful explicit cleanup prevents
another teardown on drop. Failed explicit cleanup leaves the guard active so
drop attempts cleanup again before returning the original failure.

A failed enable can leave reporting partially active. Both the free function
and guard constructor attempt best-effort teardown before returning the
original enable error, even if cleanup also fails. `MouseError` distinguishes
`EnableReporting` from `DisableReporting`, and its error source retains the
underlying I/O error for callers to inspect.

Use one reporting owner at a time. Guards are not a nested restoration stack.
Pausing for a child program means a full teardown followed by a fresh enable;
no tracking mode is reset between setup and teardown.

## Stream and platform boundary

On macOS and Linux, setup writes exactly modes 1000, 1002, and 1006 in that
order, then flushes. This requests press/release events, drag motion while a
button is held, and SGR coordinates. It never enables hover mode 1003.
Teardown writes all eight resets, in order: 9, 1000, 1002, 1003, 1005, 1006,
1015, and 1016, then flushes. The exported `ENABLE_MOUSE_REPORTING` and
`DISABLE_MOUSE_REPORTING` constants contain those complete byte sequences.
Short writes are completed with `write_all`. Teardown is safe before setup
and repeated teardown emits the full reset sequence again.

These writes use only the supplied drawing stream. The crate never chooses
stdout, stderr, or a controlling terminal on the app's behalf, since a
different stream could be redirected away from the UI.

On Windows, the platform module directly invokes crossterm 0.29's
`EnableMouseCapture` and `DisableMouseCapture` console operations. It neither
writes nor flushes the supplied writer and does not use the commands' ANSI
representations. Crossterm saves the first console input mode process-wide
and restores that value on disable, rather than capturing a fresh snapshot
for each guard. Disabling before crossterm has initialized that snapshot may
return a typed I/O error; it does not panic. The host pseudo-console can
request hover reports independently, as described in
[the terminal measurements](terminals.md).

All stream writes, flushes, and console operations live in
`crates/ratatui-mouse/src/modes/platform.rs`; lifecycle and error handling are
shared across platforms.

## Panic cleanup belongs to the app

The crate does not install or replace a panic hook. Apps using it directly
must install their own hook and perform best-effort teardown through
`disable_mouse_reporting` on the drawing stream. On macOS and Linux a hook
can also write and flush `DISABLE_MOUSE_REPORTING` itself. On Windows it must
use the function so crossterm restores the console input mode. Coordinate
writer ownership with the hook, since an unwinding panic runs the hook before
it drops local guards. A second teardown during unwinding is safe.

## Dependencies and verification

There are no default features. Shared ratatui and crossterm versions keep app
and toolkit types compatible. `unicode-segmentation` finds grapheme boundaries
for app-held-text selection. The optional `vt100` dependency is enabled only
by the `vt100` feature for emulator-backed surfaces. It exposes
`Vt100Surface` and `PaneMouseRequest::from_screen` for reading a pane
program's tracking, encoding, alternate-screen, and application-cursor modes.

The modes integration tests exercise exact bytes, short writes, write and
flush error sources, cleanup after partial enable, and the guard's drop and
explicit-cleanup paths on stream-based platforms. Cross-target lint checks
the Windows console implementation. Live terminal behavior and Windows
console-state restoration are verified by the M1 terminal integration tasks,
including Task 17, rather than by treating a redirected writer on macOS as a
Windows console.

## Frame geometry and content coordinates

Keep a `HitAreas<T>` in app state, clear it before drawing, then push each
interactive content rectangle as it is drawn. Later pushes win, including
modal overlays and backdrops. Empty rectangles are ignored. Lookup returns
borrowed targets without requiring them to be copied or cloned; `area_of`
requires only equality and returns the target's most recent area. Iteration
keeps drawing order from bottom to top.

`clamp_position_into` returns the nearest cell in the full area and signed
vertical overshoot: negative above, positive below. A nonzero edge margin
counts the outer rows as outside while keeping the returned cell inside the
original area. Margins are capped at the visible height. When margins
overlap, the nearer physical edge wins, with ties toward the top. Thus both
rows of a two-row pane with margin one remain active scrolling edges. Empty
areas, including rectangles with no representable cells at coordinate limits,
return `None`.

`ContentPoint` stores a signed content line and a cell column. Screen mapping
adds the screen offset within the content rectangle to both axes of the
surface's origin. Reverse mapping returns `None` outside the viewport; it
handles extreme origins without overflowing. Forward mapping requires a
screen cell inside the area and saturates at the content types' numeric limits
if the translated coordinate cannot be represented. `ContentRange::new`
orders either pair of endpoints in reading order. Containment includes both
ends, and union takes the smallest range containing both ranges.

## Click runs and settings

`ClickCounter<T>` accepts a target, screen position, caller-provided `Instant`,
and `MouseSettings` for each left press. Counts cycle through 1, 2, 3, 1.
The next press continues only when its target matches the previous target,
both cell distances are within the configured tolerance, and its time is no
more than the configured interval after the previous press. The default
boundaries of 500 ms and one cell are inclusive. `reset` starts a fresh run,
for example after a drag. Targets need only `Clone + PartialEq`; the library
never reads a clock.

`MouseSettings::default` supplies the specification's defaults: wheel steps
of three rows, the click limits above, Shift or Alt for selection, 50 ms edge
scrolling with at most ten rows per step and no edge margin, and a 16 ms
redraw interval. Copy on release, keeping copied selections, clearing on key
press or resize, pass-through, wheel arrows, and built-in select-mode keys
are enabled. Pure pane routing consumes the routing settings; controller
gestures consume selection and copy settings. Wheel routing, timed edge
scrolling, and redraws consume their corresponding settings; keyboard select
mode arrives in a later M1 part.
`WordCharacters` recognizes Unicode letters and digits plus configured
punctuation, defaulting to `_-./~:@+%#?&=` so paths and URLs form words.

The geometry and click integration tests exercise stacking, empty and extreme
rectangles, edge-margin overlap, both origin axes, inclusive ranges, click
thresholds and resets, and custom word punctuation. These primitives perform
no terminal I/O and require only the pure-logic test layers.

## Cell-aware text and borrowed surfaces

The `words` functions work in inclusive cell columns, using ratatui's
`CellWidth` per grapheme, matching its buffer rendering. Selecting either half
of a wide grapheme returns the original whole cluster. Combining sequences and
emoji are copied from the original bytes without normalization. Controls and
standalone zero-width graphemes occupy no selectable cells, matching ratatui's
rendered text traversal. Word classification follows the cluster's first
character, keeping attached combining marks with their base. Default
punctuation makes paths, URLs, and `file.rs:12` single words; quotes, brackets,
and blanks are boundaries.

`SelectableSurface` uses stable content coordinates and mutable text lookups
so adapters can temporarily inspect off-screen content while preserving their
origin. `SurfaceSet<T>` borrows the surface for an app-defined target;
`NoSurfaces` handles apps with no selectable text. The trait's default
`pane_request` returns `None`. `PaneMouseRequest`, `MouseTracking`, and
`MouseEncoding` describe terminal panes' requests. The pure routing and encoding
functions below consume these requests.

`LineSurface` borrows `AsRef<str>` rows and a `ScrollPosition`. Each row is a
visual content line. `with_wrapped_rows` marks rows joined to their successor;
missing flags mean unwrapped and excess flags are ignored. A final row cannot
wrap beyond the available content. Copy joins soft wraps, inserts real
newlines, trims trailing blanks at logical-line ends, and copies ranges taller
than the viewport without scrolling. `line_at` spans all visual rows of a
logical line, including stored trailing cells. Empty rows have a column-zero
endpoint. An empty surface has an empty `line_bounds`; lookups of nonexistent
characters return the requested singleton cell, while words return `None`.

Scrolling preserves the horizontal column and returns the signed rows actually
moved at either limit. Construction clamps a stale row offset to the last full
viewport. A zero-height viewport uses one row for limits. Borrowing the adapter
ends before the app accesses its scroll state again.

### Emulator cells and retained history

With the `vt100` feature enabled, `Vt100Surface::new(&mut screen)` borrows a
terminal emulator screen and its retained scrollback. Line zero is the oldest
retained history row, followed by the live screen. The origin is the first
visible content row, and scrolling returns the signed movement actually made.
An alternate screen has no history and cannot scroll beyond its screen rows.
`pane_request` returns the program's current modes, including when tracking is
off, so the adapter is always recognized as a terminal pane.

The adapter reads the emulator's own cells, wide-character continuation flags,
and soft-wrap flags, preserving combining text and copying a wide character
whole from either cell. Text lookups can span more rows than the viewport;
temporary viewport changes, including history-size discovery, use a private
restoration guard so lookups leave the original offset intact. Logical-line
endpoints include the emulator's fixed-width padding, which is trimmed when
copying a logical line.

The constructor's mutable borrow lasts until the adapter is dropped. Drop the
adapter before processing more output or resizing, then construct it again so
its history bounds reflect the new screen. No cached history metadata survives
the borrow. Content coordinates stay on the same text as new output arrives
until history reaches capacity. After that, every evicted row shifts retained
text one line toward zero, so an existing selection can point at different
text. Clearing selections on output remains the app's policy.

### Reusing the surface contract in app tests

The reusable fixture is
[`tests/common/surface_contract.rs`](../crates/ratatui-mouse/tests/common/surface_contract.rs),
with [`line_surface.rs`](../crates/ratatui-mouse/tests/line_surface.rs) showing
how to call `assert_surface_contract(&mut dyn SelectableSurface)`. Copy that
test-only recipe into an app's test helpers, construct its surface over the
five `ROWS` with `WRAPPED_ROWS`, six columns and two visible rows, and start
scrolled to a positive origin. The checks cover copying the full history,
preserved origin axes, wide cells, logical-line joining, and actual signed
scroll counts at both ends. Logical endpoints may include adapter-specific
padding; their selected text must agree. There is no public test-support
feature or runtime dependency on this fixture.

This slice performs no terminal I/O. Host and Linux pure-logic tests and
cross-target lint apply; real terminal scenarios are exercised by later M1
integration tasks.

## Selection data, reports, and highlighting

`Selection<T>` exposes its target, inclusive content range, growth unit, and
stage through read-only accessors. Its construction and mutation stay inside
the crate for the controller. `SelectionUnit` distinguishes cells, words, and
logical lines; `SelectionStage` distinguishes an initial press, an active drag,
and a finished selection. The controller supplies these transitions as
described below.

`MouseReport<T>` carries a `MouseOutcome<T>`, an independently reported pressed
target for focus handling, and a redraw flag. Outcomes describe presses,
clicks, plain drags and releases, unused wheel movement, text for copying, or
pane input bytes. `PointerPress` preserves screen and optional content
coordinates, button, modifiers, and click count. The app performs clipboard
and pane I/O in response to these values.

Draw a highlight after rendering the surface text. `HighlightRects::from_range`
converts an inclusive content range, the surface's origin, and its content area
into at most three rectangles in screen order: a partial first row, full
middle rows, and a partial last row. A single-row range produces one rectangle.
Each part is clipped on both axes, including horizontal scrolling and numeric
coordinate limits. Empty or fully offscreen selections produce no rectangles.
The conversion uses fixed storage and does not inspect individual cells.

Ranges must already cover complete characters before conversion: the helper
has no surface from which to infer grapheme boundaries. Use the surface's
`character_at` result to snap both ends before highlighting, so a selected wide
character includes both cells. `render` patches the caller's style within the
rectangles and the buffer's bounds, preserving cell symbols and unspecified
style attributes. `DEFAULT_HIGHLIGHT_STYLE` adds reversed video without choosing
colors. These operations perform no terminal I/O and use the pure-logic test
layers.

## Pane routing and encoding

`press_route` and `wheel_route` are pure decisions for a pane request, held
modifiers, select-mode flag, and settings. Select mode, any configured
selection modifier, or disabled pass-through takes priority. Presses select
in those cases; the wheel is reported to the app for history scrolling.
Otherwise active mouse tracking forwards presses and wheel events. With
tracking off, presses select; the wheel becomes arrow keys only on the
alternate screen with wheel-to-arrows enabled, and otherwise returns to the
app. Shift or Alt alone selects with the default settings.

`build_mouse_report` converts crossterm events into one-based coordinates
relative to the pane's content area. It rejects positions outside that area,
empty areas, unsupported events, and coordinates the requested protocol cannot
represent. Default encoding supports coordinates through 223; UTF-8 supports
through 2015. SGR supports larger coordinates, retains the released button's
code, and ends releases with lowercase `m`. Default and UTF-8 releases use
button code 3. All formats preserve Shift, Alt, and Control, button-held
motion, and all four wheel directions.

Tracking filters apply independently of routing. Press-only tracking drops
releases, drags, and wheel events. Press-and-release tracking also accepts the
wheel; button-motion and any-motion tracking additionally accept drags.
Tracking off drops every event. Bare `Moved` events are always ignored,
including when a pane requested any motion: panes receive button motion on
every platform, preserving the host terminal's pointer interactions.

`build_wheel_arrow_keys` emits one up key per negative row or one down key per
positive row; zero rows emits nothing. Normal cursor mode uses `CSI A` and
`CSI B`, while application cursor mode uses `SS3 A` and `SS3 B`. All these
helpers return decisions or bytes. The app owns the controller and writes any
forwarded bytes to its child program. No helper reads or writes a terminal.

Pass-through tests cover both routing tables, custom selection modifiers,
tracking filters, protocol boundaries and empty or extreme rectangles, every
button and wheel direction with all Shift/Alt/Control combinations, both
cursor-key modes, and requests derived from a real vt100 parser. Host and
Linux pure-logic tests and cross-target lint apply; later M1 integration
scenarios exercise the forwarded bytes with real terminals.

## Controller ownership and forwarding

The app owns one `MouseController<T>`, passes it crossterm events with an
explicit `Instant`, the most recently drawn `HitAreas`, and borrowed surfaces,
then handles the returned `MouseReport`. Targets require only `Clone +
PartialEq`. Settings can change between events through `settings_mut`.

A button press fixes the gesture's owner and route. Plain presses return
`Pressed`; their drags and release keep naming the press target even over
another target. `Dragged` and `DragEnded` preserve the original press position
and clamp the end to the owner's current area. Without a drag, release returns
`Clicked` with the original `PointerPress`. A drag that returns to its starting
cell still ends as a drag, and resets the multi-click run. Only the pressed
button's drags and release advance the gesture.

Forwarding snapshots the pane's complete mouse request at the press, keeping
its tracking and encoding for the whole gesture. Later modifiers, settings,
or pane request changes cannot turn it into selection or split its protocol.
Current event modifiers are still encoded. Coordinates use the owner's
current area, with out-of-area motion clamped before pane-relative encoding.
Tracking filters can consume an event without bytes, including the release
of a press-only gesture. Every targeted press also sets `pressed_target` so
focus changes do not depend on its outcome. Bare `Moved` events are ignored,
including during an active gesture with an any-motion pane request.

The owner area is checked on every event. Removing that area, losing focus,
resizing, or explicitly cancelling drops the gesture without a click or copy.
A new press replaces an active gesture; an orphan release is ignored.
`gesture_owner`, `cancel_gesture`, and `forget_target` let apps inspect or drop
this state. Forgetting also drops that target's completed-click history so a
recreated target starts a fresh click run, while other targets' history stays.
Wheel handling and timed redraws follow the ownership rules below. Keyboard
select mode arrives in a later M1 slice.

Controller tests use non-Copy targets and cover press ownership, clicks and
click-run reset, owner relocation and disappearance, lost releases, explicit
cancellation, fixed forwarding requests, pane-relative bytes, tracking
filters, and ignored bare motion. This is pure logic with no terminal I/O:
host and Linux tests plus cross-target lint apply.

## Controller selection gestures

A nonforwarded left press on a selectable surface starts a selection owned by
that target. The initial cell press has `Pressed` state but `selected_text`
returns `None`: a release without a drag reports `Clicked` and drops that
placeholder. Apps should skip highlighting when `selected_text` returns
`None`. Once a drag arrives, the selection stays `Dragging`, even when it returns to its
starting cell. Releasing it finishes and can copy one whole character.

Double presses select a whole word immediately, falling back to the character
on punctuation or blanks outside the configured word characters. Triple
presses select the whole logical line, including all its soft-wrapped rows.
Word and line drags join the original expanded unit with the expanded unit
under the pointer. Crossing the anchor never loses the original unit. Any
drag resets the click run; a click with one cell of drift still continues it.
All stored range endpoints cover complete characters, including both cells
of a wide grapheme, before apps project highlights.

Holding any configured selection modifier, Shift or Alt by default, extends a
finished selection on its own target. Its original anchor and growth unit
remain fixed, even after a backward drag or repeated extensions. An extension
press stays `Pressed` until a drag arrives, but already exposes its selected
text and finishes on release. A press on another target starts a fresh
gesture and clears the previous selection.
Modifier changes during a gesture never change its route.

On release, empty or whitespace-only text is dropped without copying. Other
selections become `Done`. With `should_copy_on_release`, the outcome is
`Copy { target, text }`; the app owns clipboard I/O. Disabling copy keeps the
finished selection, while disabling `should_keep_selection_after_copy` clears
it after an actual copy. `selected_text` also reads active word, line, and
drag selections without finishing them.

Cancellation removes an unfinished selection without copying: focus loss,
resize, a missing owner area or surface, a replacement press, explicit
`cancel_gesture`, and `forget_target` all follow this rule. Clearing an active
selection also cancels its pending release. Current owner areas clamp motion
and release after layout changes, so selection cannot spill into another
target. A completed selection is removed if its area or surface disappears.

Outside keyboard select mode, a key press or repeat clears selections when
`should_clear_on_key_press` is enabled. Bare modifier keys and key releases
never clear them. Resize always cancels a gesture, but only clears a completed
selection when `should_clear_on_resize` is enabled. Focus loss preserves a
completed selection. These paths never emit phantom copies.

Selection controller tests use real borrowed `LineSurface` adapters and cover
stages, returning drags, wide endpoints and clipped highlights, word and
wrapped-line anchors, modifier extension, copy settings, clearing, layout
changes, and cancellation. These are pure state transitions with no terminal
I/O, so host and Linux tests and cross-target lint apply.

## Controller wheel and deadlines

Wheel direction follows crossterm without inversion. With the default step,
up and down report signed rows of -3 and +3; left and right report signed
columns of -3 and +3. An unused wheel reports the target under the pointer,
regardless of focus; input outside all targets is ignored. A completed
selection stays fixed
in content coordinates while the app handles wheel scrolling.

During a selection gesture the controller scrolls the owner's surface by the
vertical step and updates the selection endpoint under the pointer, clamped
to the current owner area. It returns `Consumed`, including horizontal input:
surfaces expose vertical scrolling only. During a forwarded gesture, wheel
input uses that owner's area and press-time tracking and encoding request.
Without either gesture, a pane follows `wheel_route`: selection modifiers or
disabled pass-through report to the app, requested tracking forwards mouse
bytes, and an alternate screen with tracking off can receive vertical arrow
keys. Horizontal input remains a `Wheel` report when that route offers only
vertical arrows.

An out-of-area selection drag records a pointer and schedules an edge step
at its supplied `now`; the drag itself never scrolls. Further drag events
update distance without moving that deadline. `tick(now, areas, surfaces)`
performs at most one due step, bounded by overshoot and `auto_scroll_max_rows`,
then updates the content endpoint. Continued work is scheduled relative to
that tick, 50 ms later by default, so a late tick does not replay missed
steps. Edge margins use the same clamping rule as frame geometry, including
one-row areas. Returning inside, release, cancellation, owner removal, a
missing surface, and resize clear edge work. A zero or partial movement at a
content boundary stops scheduling until another drag.

Selection changes, including cancellation and explicit clearing, and actual
surface movement request redraws. The first change is due immediately;
subsequent changes inside the default 16 ms interval are remembered. A due
event or tick delivers one redraw and clears the pending flag. Unchanged
drags and bare motion add no redraw work. `next_deadline()` returns the
earlier of pending redraw and edge work, letting the app bound its input
wait. Direct clearing has no timestamp, so its redraw uses the next supplied
event or tick and the last redraw's budget.

All timing uses caller-supplied `Instant` values. The library never reads a
clock, starts a thread, or sleeps. Zero edge maxima disable edge steps; zero
intervals allow one step or redraw per call without loops. An interval that
cannot be added to an `Instant` schedules no deadline, avoiding overflow or
immediate retry loops. Apps can change settings before subsequent calls.

Controller timing tests use borrowed text surfaces and exact timestamps to
cover each wheel direction and route, owner snapshots, endpoint changes,
redraw coalescing, nearest deadlines, cancellation, content boundaries, late
ticks, and zero or extreme settings. These changes perform no terminal I/O;
host and Linux pure-logic tests plus Windows/Linux cross-target lint apply.
